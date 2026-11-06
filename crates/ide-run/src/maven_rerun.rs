use crate::{RunConfig, process::Invocation};
use ide_core::{IdeError, IdeResult};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(default)]
pub struct MavenModuleTests {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[specta(optional)]
    pub surefire: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[specta(optional)]
    pub failsafe: Option<String>,
}

const EXTENSIONS: &str = "maven.ext.class.path";

pub(crate) fn prepare(
    root: &Path,
    cwd: &Path,
    config: &RunConfig,
    inv: &mut Invocation,
    env: &BTreeMap<String, String>,
) -> IdeResult<tempfile::TempDir> {
    if inv.shell
        || (config.kind != "maven"
            && !matches!(crate::wrap::program_name(&config.command).as_str(), "mvn" | "mvnw" | "mvnd"))
    {
        return Err(IdeError::invalid("Reactor test selection requires a direct Maven command"));
    }

    let mut contents = Vec::new();
    for (module, tests) in &config.extras.maven_test_selection {
        let dir = dunce::canonicalize(root.join(module))
            .map_err(|e| IdeError::invalid(format!("Cannot resolve selected Maven module '{module}': {e}")))?;
        for field in [
            dir.to_str().ok_or_else(|| IdeError::invalid("Maven module path is not UTF-8"))?,
            tests.surefire.as_deref().unwrap_or_default(),
            tests.failsafe.as_deref().unwrap_or_default(),
        ] {
            if field.contains('\0') {
                return Err(IdeError::invalid("Invalid Maven test selection"));
            }
            contents.extend_from_slice(field.as_bytes());
            contents.push(0);
        }
    }

    let files = tempfile::Builder::new().prefix("openidea-maven-tests-").tempdir()?;
    let extension = files.path().join("test-scope.jar");
    let selection = files.path().join("selection");
    std::fs::write(&extension, include_bytes!("../../../java/maven-test-scope/openidea-maven-test-scope.jar"))?;
    std::fs::write(&selection, contents)?;

    let existing = extension_path(cwd, &inv.args, env);
    let paths = existing.as_deref().filter(|value| !value.is_empty()).into_iter().flat_map(std::env::split_paths);
    let merged = std::env::join_paths(paths.chain([extension]))
        .map_err(|e| IdeError::invalid(format!("Cannot extend the Maven extension classpath: {e}")))?;
    let boundary = inv.args.iter().position(|arg| arg == "--").unwrap_or(inv.args.len());
    inv.args.splice(
        boundary..boundary,
        [
            format!("-D{EXTENSIONS}={}", merged.to_string_lossy()),
            format!("-Dopenidea.maven.testSelection={}", selection.display()),
        ],
    );
    Ok(files)
}

fn property(args: &[String]) -> Option<String> {
    let mut found = None;
    let mut args = args.iter().map(String::as_str);
    while let Some(arg) = args.next().filter(|arg| *arg != "--") {
        let definition = match arg {
            "-D" | "--define" => args.next(),
            _ => arg.strip_prefix("-D").or_else(|| arg.strip_prefix("--define=")),
        };
        if let Some((_, value)) =
            definition.map(|arg| arg.split_once('=').unwrap_or((arg, "true"))).filter(|(key, _)| *key == EXTENSIONS)
        {
            found = Some(value.to_owned());
        }
    }
    found
}

fn extension_path(cwd: &Path, args: &[String], env: &BTreeMap<String, String>) -> Option<String> {
    let basedir = project_base(cwd, args, env.get("MAVEN_BASEDIR").map(String::as_str));
    // CLI user properties override maven.config, which overrides JVM properties.
    property(args)
        .or_else(|| property(&env_words(env, "MAVEN_ARGS")))
        .or_else(|| property(&config_args(&basedir)))
        .or_else(|| property(&env_words(env, "_JAVA_OPTIONS")))
        .or_else(|| property(&env_words(env, "MAVEN_DEBUG_OPTS")))
        .or_else(|| property(&env_words(env, "MAVEN_OPTS")))
        .or_else(|| {
            let text = std::fs::read_to_string(basedir.join(".mvn/jvm.config")).ok()?;
            property(&option_words(&text))
        })
        .or_else(|| property(&env_words(env, "JDK_JAVA_OPTIONS")))
        .or_else(|| property(&env_words(env, "JAVA_TOOL_OPTIONS")))
}

fn env_words(env: &BTreeMap<String, String>, key: &str) -> Vec<String> {
    env.get(key).map_or_else(Vec::new, |value| option_words(value))
}

fn option_words(value: &str) -> Vec<String> {
    if cfg!(windows) {
        // Windows uses backslashes in paths, where the shared shell parser expects escapes.
        crate::import::split_args(&value.replace('\\', "/"))
    } else {
        crate::import::split_args(value)
    }
}

/// The launcher's .mvn search starts at a separate -f/--file argument or the working directory.
pub fn project_base(cwd: &Path, args: &[String], override_dir: Option<&str>) -> PathBuf {
    if let Some(dir) = override_dir.filter(|dir| !dir.is_empty()) {
        return cwd.join(dir);
    }
    let start = args
        .windows(2)
        .take_while(|pair| pair[0] != "--")
        .find(|pair| matches!(pair[0].as_str(), "-f" | "--file"))
        .map(|pair| cwd.join(&pair[1]))
        .map(|path| if path.is_dir() { path } else { path.parent().unwrap_or(cwd).to_path_buf() });
    let start = start.as_deref().unwrap_or(cwd);
    start.ancestors().find(|dir| dir.join(".mvn").is_dir()).unwrap_or(start).to_path_buf()
}

/// Maven 3.9 takes one argument per line. Also read flag/value pairs from older config files.
pub fn config_args(base: &Path) -> Vec<String> {
    std::fs::read_to_string(base.join(".mvn/maven.config"))
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .flat_map(|line| {
            if line
                .split_once(char::is_whitespace)
                .is_some_and(|(flag, _)| flag.starts_with('-') && !flag.contains('='))
            {
                crate::import::split_args(line)
            } else {
                vec![line.to_owned()]
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_the_effective_maven_extension_path() {
        let temp = tempfile::tempdir().unwrap();
        let reactor = temp.path().join("other reactor");
        std::fs::create_dir_all(reactor.join(".mvn")).unwrap();
        std::fs::write(reactor.join(".mvn/maven.config"), "-Dmaven.ext.class.path=config extension.jar\n").unwrap();
        let mut args = vec!["-f".into(), "other reactor/build.xml".into(), "verify".into()];
        let mut env = BTreeMap::from([("MAVEN_OPTS".into(), "-Dmaven.ext.class.path=jvm.jar".into())]);
        let effective = |args: &[String], env: &BTreeMap<String, String>| extension_path(temp.path(), args, env);
        assert_eq!(effective(&args, &env).as_deref(), Some("config extension.jar"));
        env.insert("MAVEN_ARGS".into(), "-Dmaven.ext.class.path=env.jar".into());
        assert_eq!(effective(&args, &env).as_deref(), Some("env.jar"));
        args.extend(["--define".into(), "maven.ext.class.path=cli.jar".into(), "--".into()]);
        args.push("-Dmaven.ext.class.path=after-boundary.jar".into());
        assert_eq!(effective(&args, &env).as_deref(), Some("cli.jar"));
    }
}
