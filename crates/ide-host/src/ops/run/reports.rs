use ide_run::RunConfig;
use ide_run::junit::{MAVEN_RUN_PROPERTY, MavenReportModule, MavenTestPlugin, ReportPlan};
use ide_run::wrap;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

enum Tool {
    Maven,
    Gradle,
}

fn tool(config: &RunConfig) -> Option<Tool> {
    match wrap::program_name(config.command.trim()).as_str() {
        "mvn" | "mvnw" | "mvnd" => Some(Tool::Maven),
        "gradle" | "gradlew" => Some(Tool::Gradle),
        _ => match config.kind.as_str() {
            "maven" => Some(Tool::Maven),
            "gradle" => Some(Tool::Gradle),
            _ => None,
        },
    }
}

pub(super) fn applies(config: &RunConfig) -> bool {
    config.is_test && tool(config).is_some()
}

pub(super) fn prepare(root: &Path, config: &mut RunConfig, mut scopes: Vec<PathBuf>) -> Option<ReportPlan> {
    let cwd = wrap::run_dir(root, config);
    match tool(config)? {
        Tool::Gradle => return Some(ReportPlan::Gradle { cwd }),
        Tool::Maven => {}
    }

    if wrap::uses_shell(config, &cwd) || (config.detected && config.id.starts_with(ide_run::EXTERNAL_PREFIX)) {
        // The direct Maven arguments are unavailable for shell or plugin launchers.
        return Some(ReportPlan::Maven {
            modules: vec![MavenReportModule { dir: dunce::canonicalize(cwd).ok()?, report_dirs: Vec::new() }],
            root: root.to_path_buf(),
            marker: None,
            reactor: false,
        });
    }

    let env = wrap::run_env(root, config).ok()?;
    let env_args = env.get("MAVEN_ARGS").map_or_else(Vec::new, |s| ide_run::import::split_args(s));
    let cli: Vec<&str> = env_args.iter().chain(&config.args).map(String::as_str).collect();
    let cli_file = file_argument(&cli);
    let base = ide_run::maven_rerun::project_base(&cwd, &config.args, env.get("MAVEN_BASEDIR").map(String::as_str));
    let file_args = ide_run::maven_rerun::config_args(&base);
    let file_args: Vec<&str> = file_args.iter().map(String::as_str).collect();
    let pom = pom_path(&cwd, cli_file.or_else(|| file_argument(&file_args)).unwrap_or("pom.xml"));
    let pom = dunce::canonicalize(pom).ok()?;
    let dir = pom.parent()?;

    scopes.push(root.to_path_buf());
    if !scopes.iter().any(|scope| dir.starts_with(scope)) {
        // An explicitly chosen POM/working directory can be outside the opened project.
        scopes.push(dir.to_path_buf());
    }
    let mut profiles = Vec::new();
    let mut properties: HashMap<String, String> = env.iter().map(|(k, v)| (format!("env.{k}"), v.clone())).collect();
    properties.insert("maven.multiModuleProjectDirectory".into(), base.to_string_lossy().into_owned());
    for args in [&file_args, &cli] {
        profiles.extend(
            option_values(args, &["-P", "--activate-profiles"], Some("-P"))
                .flat_map(|p| p.split(',').map(|p| p.trim().trim_start_matches('?').to_owned())),
        );
        properties.extend(option_values(args, &["-D", "--define"], Some("-D")).filter_map(|p| {
            let (k, v) = p.split_once('=').unwrap_or((p, "true"));
            (!k.is_empty()).then(|| (k.to_owned(), v.to_owned()))
        }));
    }
    let local_repo = properties.get("maven.repo.local").map(|p| cwd.join(p)).or_else(|| {
        option_values(&cli, &["-s", "--settings"], None)
            .chain(option_values(&file_args, &["-s", "--settings"], None))
            .next()
            .and_then(|p| ide_build::maven::local_repository_of(&cwd.join(p)))
    });
    let candidates = ide_build::maven::test_reactor_modules(&pom, &scopes, local_repo, &profiles, &properties);
    let needs_jvm_marker = candidates.modules.iter().any(|m| m.needs_jvm_marker);
    let mut modules: Vec<_> = candidates
        .modules
        .into_iter()
        .map(|m| MavenReportModule {
            dir: m.dir,
            report_dirs: m
                .surefire_dirs
                .into_iter()
                .map(|path| (path, MavenTestPlugin::Surefire))
                .chain(m.failsafe_dirs.into_iter().map(|path| (path, MavenTestPlugin::Failsafe)))
                .collect(),
        })
        .collect();
    if modules.is_empty() {
        modules.push(MavenReportModule { dir: dir.to_path_buf(), report_dirs: Vec::new() });
    }

    // Only reports tagged by this Maven invocation can be attributed to it.
    let epoch = std::time::UNIX_EPOCH.elapsed().unwrap_or_default().as_nanos();
    let marker = format!("{}-{epoch}-{}", std::process::id(), ide_core::RunId::next().0);
    let option = format!("-D{MAVEN_RUN_PROPERTY}={marker}");
    if needs_jvm_marker {
        // With promotion disabled the fork needs the property at JVM startup.
        // Preserve the user's argLine and effective JAVA_TOOL_OPTIONS.
        let key = "JAVA_TOOL_OPTIONS";
        let existing = env.get(key).map_or("", String::as_str).trim_end();
        let options = if existing.is_empty() { option.clone() } else { format!("{existing} {option}") };
        config.env.insert(key.into(), options);
    }
    config.args.insert(0, option);
    Some(ReportPlan::Maven { modules, root: root.to_path_buf(), marker: Some(marker), reactor: candidates.has_modules })
}

fn pom_path(cwd: &Path, file: &str) -> PathBuf {
    let path = cwd.join(file);
    if path.is_dir() { path.join("pom.xml") } else { path }
}

fn file_argument<'a>(args: &[&'a str]) -> Option<&'a str> {
    let args: Vec<_> = args.iter().copied().filter(|a| !matches!(*a, "-fae" | "-ff" | "-fn")).collect();
    option_values(&args, &["-f", "--file"], Some("-f")).next()
}

fn option_values<'a, 'b>(
    args: &'b [&'a str],
    names: &'b [&str],
    attached: Option<&'b str>,
) -> impl Iterator<Item = &'a str> + 'b
where
    'a: 'b,
{
    let mut args = args.iter().copied();
    std::iter::from_fn(move || {
        while let Some(arg) = args.next().filter(|a| *a != "--") {
            if names.contains(&arg) {
                return args.next();
            }
            if let Some((_, value)) = arg.split_once('=').filter(|(flag, _)| names.contains(flag)) {
                return Some(value);
            }
            if let Some(value) = attached.and_then(|prefix| arg.strip_prefix(prefix)) {
                return Some(value);
            }
        }
        None
    })
}
