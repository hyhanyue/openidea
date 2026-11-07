//! Consume the first-party transient launch reference; recover saved values only inside its project.
use openidea_plugin::{Project, host};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fact {
    Known(String),
    Unknown,
}
impl Fact {
    pub fn value(&self) -> Option<&str> {
        if let Self::Known(value) = self { Some(value) } else { None }
    }
}
#[derive(Default)]
pub struct Launch {
    pub name: Option<String>,
    pub main: String,
    pub module: String,
    pub cwd: String,
    pub env: BTreeMap<String, Fact>,
    pub env_files: BTreeMap<String, Fact>,
    pub system: BTreeMap<String, Fact>,
    pub cli: BTreeMap<String, Fact>,
    pub inherited: bool,
    pub unknown_selection: bool,
    pub unresolved: BTreeSet<String>,
    pub warnings: BTreeSet<String>,
}
fn facts(value: Option<&Value>) -> BTreeMap<String, Fact> {
    value
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
        .map(|(key, value)| {
            let fact = if value.get("state").and_then(Value::as_str) == Some("known") {
                value.get("value").and_then(Value::as_str).map(|s| Fact::Known(s.into())).unwrap_or(Fact::Unknown)
            } else {
                Fact::Unknown
            };
            (key.clone(), fact)
        })
        .collect()
}
fn text<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}
fn strings(value: Option<&Value>) -> Vec<String> {
    value.and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).map(str::to_owned).collect()
}
pub fn read(project: &Project) -> Launch {
    let mut out = Launch { inherited: true, cwd: ".".into(), ..Launch::default() };
    let Some(raw) = host::setting("springBeans.launchContext") else { return out };
    let input: Option<Value> = serde_json::from_str(&raw).ok();
    if input.as_ref().is_some_and(Value::is_null) {
        return out;
    }
    let Some(input) = input.filter(|v| v.get("version").and_then(Value::as_u64) == Some(1)) else {
        out.unknown_selection = true;
        return out;
    };
    let reference = &input["launch"];
    if !reference.is_object() {
        out.unknown_selection = true;
        return out;
    }
    out.name = text(reference, "name").map(str::to_owned);
    out.main = text(reference, "mainClass").unwrap_or_default().into();
    out.module = text(reference, "module").unwrap_or_default().into();
    out.cwd = text(reference, "cwd").unwrap_or(".").into();
    out.inherited = reference.get("inheritEnvironment").and_then(Value::as_bool).unwrap_or(true);
    out.env = facts(reference.get("environment"));
    out.system = facts(reference.get("systemProperties"));
    out.cli = facts(reference.get("commandLineProperties"));
    out.unresolved = strings(reference.get("unresolvedSources")).into_iter().collect();
    let mut recovered = false;
    if reference.get("persisted").and_then(Value::as_bool) == Some(true) {
        if let Some(saved) = saved(project, reference) {
            out.env = saved.env;
            out.system = saved.system;
            out.cli = saved.cli;
            out.read_env_files(project, &saved.env_files);
            recovered = true;
        } else {
            out.warnings.insert("The selected saved launch could not be matched; redacted facts remain unknown".into());
        }
    }
    if !recovered && !strings(reference.get("envFiles")).is_empty() {
        out.unresolved.insert("envFiles".into());
    }
    if out.inherited {
        out.unresolved.insert("inheritedEnvironment".into());
    }
    // JVM option variables may be sourced from inherited state. Never enumerate the actual process environment.
    let option_keys = ["JAVA_TOOL_OPTIONS", "JDK_JAVA_OPTIONS", "_JAVA_OPTIONS"];
    if !out.inherited
        && !out.unresolved.contains("envFiles")
        && !option_keys.iter().any(|key| out.env.contains_key(*key) || out.env_files.contains_key(*key))
    {
        out.unresolved.remove("implicitJvmOptions");
    }
    out
}
#[derive(Default)]
struct Saved {
    main: String,
    module: Option<String>,
    cwd: Option<String>,
    env: BTreeMap<String, Fact>,
    system: BTreeMap<String, Fact>,
    cli: BTreeMap<String, Fact>,
    env_files: Vec<String>,
    inherited: bool,
}
fn value_text<'a>(value: &'a toml::Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(toml::Value::as_str)
}
fn value_strings(value: Option<&toml::Value>) -> Option<Vec<String>> {
    value
        .and_then(toml::Value::as_array)
        .map(|list| list.iter().map(|v| v.as_str().map(str::to_owned)).collect())
        .unwrap_or(Some(vec![]))
}
fn property(args: &[String], key: &str) -> Option<String> {
    args.iter().rev().find_map(|s| s.strip_prefix(&format!("-D{key}=")).map(str::to_owned))
}
fn words(text: &str) -> Option<Vec<String>> {
    let mut out = vec![];
    let mut word = String::new();
    let mut quote = None;
    let mut started = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match quote {
            Some(q) if c == q => quote = None,
            Some('"') if c == '\\' && chars.peek().is_some_and(|n| "\"\\$`".contains(*n)) => {
                word.push(chars.next()?);
            }
            Some(_) => word.push(c),
            None if c == '\'' || c == '"' => {
                quote = Some(c);
                started = true;
            }
            None if c == '\\' => {
                word.push(chars.next()?);
                started = true;
            }
            None if c.is_whitespace() => {
                if started || !word.is_empty() {
                    out.push(std::mem::take(&mut word));
                }
                started = false;
            }
            None => {
                word.push(c);
                started = true;
            }
        }
    }
    if quote.is_some() {
        return None;
    }
    if started || !word.is_empty() {
        out.push(word);
    }
    Some(out)
}
fn evaluated(value: &str) -> Fact {
    if value.contains(['$', '`']) { Fact::Unknown } else { Fact::Known(value.into()) }
}
fn parsed(value: &toml::Value) -> Option<Saved> {
    let kind = value_text(value, "kind")?;
    let args = value_strings(value.get("args"))?;
    let mut out = Saved {
        module: value_text(value, "module").map(str::to_owned),
        cwd: value_text(value, "cwd").map(str::to_owned),
        env_files: value_strings(value.get("env-files").or_else(|| value.get("envFiles")))?,
        inherited: value
            .get("include-system-env")
            .or_else(|| value.get("includeSystemEnv"))
            .and_then(toml::Value::as_bool)
            .unwrap_or(true),
        ..Saved::default()
    };
    for (key, value) in value.get("env").and_then(toml::Value::as_table).into_iter().flatten() {
        out.env.insert(key.clone(), evaluated(value.as_str()?));
    }
    let (vm, program) = if ["application", "spring-boot"].contains(&kind) {
        let mut vm = vec![];
        let mut index = 0;
        while index < args.len() {
            let arg = &args[index];
            if [
                "-cp",
                "-classpath",
                "--class-path",
                "-p",
                "--module-path",
                "--upgrade-module-path",
                "--add-modules",
                "--add-opens",
                "--add-exports",
                "--add-reads",
                "--patch-module",
                "--limit-modules",
                "--enable-native-access",
            ]
            .contains(&arg.as_str())
            {
                index += 2;
                continue;
            }
            if arg.starts_with('-') {
                vm.push(arg.clone());
                index += 1;
            } else {
                break;
            }
        }
        if let Some(main) = args.get(index) {
            out.main = main.clone();
        }
        (vm, args.get(index + 1..).unwrap_or_default().to_vec())
    } else if kind == "maven"
        && args
            .iter()
            .any(|a| a == "spring-boot:run" || a.contains(":spring-boot-maven-plugin:") && a.ends_with(":run"))
    {
        out.main = property(&args, "spring-boot.run.main-class").unwrap_or_default();
        out.module = out.module.or_else(|| Some(out.cwd.clone().unwrap_or_default()));
        out.cwd = property(&args, "spring-boot.run.workingDirectory").or(out.cwd);
        let vm = words(&property(&args, "spring-boot.run.jvmArguments").unwrap_or_default())?;
        let mut program = words(&property(&args, "spring-boot.run.arguments").unwrap_or_default())?;
        if let Some(profiles) = property(&args, "spring-boot.run.profiles") {
            program.insert(0, format!("--spring.profiles.active={profiles}"));
        }
        (vm, program)
    } else {
        return None;
    };
    for arg in vm {
        if let Some((key, value)) = arg.strip_prefix("-D").and_then(|s| s.split_once('=')) {
            out.system.insert(key.into(), evaluated(value));
        }
    }
    let mut cli: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for arg in program {
        if let Some(arg) = arg.strip_prefix("--") {
            let (key, value) = arg.split_once('=').unwrap_or((arg, ""));
            cli.entry(key.into()).or_default().push(value.into());
        }
    }
    out.cli = cli.into_iter().map(|(key, values)| (key, evaluated(&values.join(",")))).collect();
    Some(out)
}
fn matches_facts(reference: Option<&Value>, actual: &BTreeMap<String, Fact>) -> bool {
    let reference = facts(reference);
    reference.len() == actual.len()
        && reference
            .iter()
            .all(|(key, fact)| actual.get(key).is_some_and(|value| matches!(fact, Fact::Unknown) || fact == value))
}
fn saved(project: &Project, reference: &Value) -> Option<Saved> {
    let root = project.root().ok()?;
    let id = text(reference, "configId")?;
    for name in ["run.toml", "run.local.toml"] {
        let Ok(raw) = project.read_file(&format!("{root}/.openidea/{name}")) else { continue };
        if raw.len() > 1024 * 1024 {
            return None;
        }
        let doc: toml::Value = toml::from_str(&raw).ok()?;
        let candidates: Vec<_> = doc
            .get("configurations")
            .and_then(toml::Value::as_array)
            .into_iter()
            .flatten()
            .filter(|c| value_text(c, "id") == Some(id))
            .collect();
        if candidates.is_empty() {
            continue;
        }
        if candidates.len() != 1 {
            return None;
        }
        let config = candidates[0];
        if value_text(config, "kind") != text(reference, "kind") {
            return None;
        }
        let saved = parsed(config)?;
        if saved.main != text(reference, "mainClass").unwrap_or_default()
            || saved.module.as_deref() != text(reference, "module")
            || saved.cwd.as_deref() != text(reference, "cwd")
            || saved.inherited != reference.get("inheritEnvironment").and_then(Value::as_bool).unwrap_or(true)
            || saved.env_files != strings(reference.get("envFiles"))
            || !matches_facts(reference.get("environment"), &saved.env)
            || !matches_facts(reference.get("systemProperties"), &saved.system)
            || !matches_facts(reference.get("commandLineProperties"), &saved.cli)
        {
            return None;
        }
        return Some(saved);
    }
    None
}
impl Launch {
    fn read_env_files(&mut self, project: &Project, paths: &[String]) {
        self.unresolved.remove("envFiles");
        let root = project.root().unwrap_or_default();
        let mut bytes = 0;
        for path in paths.iter().take(128) {
            let Some(path) = crate::environment::local(&root, &root, path) else {
                self.unresolved.insert("envFiles".into());
                continue;
            };
            let Ok(raw) = project.read_file(&path) else {
                self.unresolved.insert("envFiles".into());
                continue;
            };
            bytes += raw.len();
            if raw.len() > 1024 * 1024 || bytes > 8 * 1024 * 1024 {
                self.unresolved.insert("envFiles".into());
                continue;
            }
            for line in raw.lines() {
                if self.env_files.len() >= 8192 {
                    self.unresolved.insert("envFiles".into());
                    break;
                }
                let line = line.trim().strip_prefix("export ").unwrap_or(line.trim());
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                let Some((key, value)) = line.split_once('=') else {
                    self.unresolved.insert("envFiles".into());
                    continue;
                };
                let key = key.trim();
                let value = value.trim();
                if key.is_empty() || !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                    self.unresolved.insert("envFiles".into());
                    continue;
                }
                let value = if let Some(q @ ('\'' | '"')) = value.chars().next() {
                    value.strip_prefix(q).and_then(|s| s.strip_suffix(q)).map(str::to_owned)
                } else {
                    Some(value.split(" #").next().unwrap_or(value).trim_end().to_owned())
                };
                let fact = value.filter(|s| !s.contains(['\\', '$', '`'])).map(Fact::Known).unwrap_or(Fact::Unknown);
                self.env_files.insert(key.into(), fact);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::environment::{self, State};
    use openidea_plugin::mock::MockHost;
    const CONTRACT: &str = include_str!("../../../../fixtures/java-spring-configuration/launch-context-contract.json");
    #[test]
    fn shared_producer_consumer_contract_and_selection_sequence() {
        let contract: Value = serde_json::from_str(CONTRACT).unwrap();
        assert_eq!(contract["version"], 1);
        let fixtures = contract["fixtures"].as_array().unwrap();
        for name in contract["sequence"].as_array().unwrap().iter().filter_map(Value::as_str) {
            // Each fixture has its own project to keep the filesystem facts independent.
            environment::invalidate();
            let host = MockHost::install();
            let project = host.invocation().project().unwrap();
            let case = fixtures.iter().find(|c| c["name"] == name).unwrap();
            for (path, text) in case["projectFiles"].as_object().unwrap() {
                host.add_file(path, text.as_str().unwrap());
            }
            host.set_setting("springBeans.launchContext", &case["producerExpected"].to_string());
            let snapshot = environment::snapshot_project(&project, &BTreeMap::new(), None);
            let expected = &case["consumerExpected"];
            if let Some(profiles) = expected.get("profiles").and_then(Value::as_array) {
                assert_eq!(
                    snapshot.profile_order,
                    profiles.iter().filter_map(Value::as_str).collect::<Vec<_>>(),
                    "{name}"
                );
            }
            for (key, wanted) in expected.get("properties").and_then(Value::as_object).into_iter().flatten() {
                let resolved = snapshot.resolve(key);
                if wanted["state"] == "known" {
                    assert_eq!(resolved.state, State::Value(wanted["value"].as_str().unwrap().into()), "{name}: {key}");
                } else {
                    assert!(matches!(resolved.state, State::Unknown(_)), "{name}: {key}");
                }
                let source = snapshot.candidates(key).last().unwrap().origin.clone();
                assert_eq!(
                    source,
                    match wanted["source"].as_str().unwrap() {
                        "commandLine" => "Command-line argument",
                        "environment" => "Launch environment",
                        other => other,
                    },
                    "{name}"
                );
            }
            if name == "deselect-clears-launch" {
                assert!(snapshot.launch.is_none());
                assert!(snapshot.profiles.is_none());
            }
            if let Some(id) = expected["pluginId"].as_str() {
                assert!(contract["transport"]["pluginIds"].as_array().unwrap().iter().any(|v| v == id));
            }
        }
        // Deselect the same live project after a known launch; neither its facts nor cache survive.
        let host = MockHost::install();
        let project = host.invocation().project().unwrap();
        let case = fixtures.iter().find(|c| c["name"] == "dev").unwrap();
        for (path, text) in case["projectFiles"].as_object().unwrap() {
            host.add_file(path, text.as_str().unwrap());
        }
        host.set_setting("springBeans.launchContext", &case["producerExpected"].to_string());
        assert!(environment::snapshot_project(&project, &BTreeMap::new(), None).profiles.unwrap().contains("dev"));
        host.set_setting("springBeans.launchContext", "null");
        let out = environment::snapshot_project(&project, &BTreeMap::new(), None);
        assert!(out.launch.is_none());
        assert!(out.profiles.is_none());
        host.set_setting("springBeans.launchContext", r#"{"version":1,"launch":null}"#);
        assert!(matches!(
            environment::snapshot_project(&project, &BTreeMap::new(), None).resolve("feature.enabled").state,
            State::Unknown(_)
        ));
    }
    #[test]
    fn unmatched_saved_reference_never_borrows_a_config_and_null_is_not_absence() {
        let host = MockHost::install();
        let project = host.invocation().project().unwrap();
        environment::invalidate();
        let contract: Value = serde_json::from_str(CONTRACT).unwrap();
        let case =
            contract["fixtures"].as_array().unwrap().iter().find(|c| c["name"] == "prod-persisted-nonboolean").unwrap();
        for (path, text) in case["projectFiles"].as_object().unwrap() {
            host.add_file(path, text.as_str().unwrap());
        }
        let mut context = case["producerExpected"].clone();
        context["launch"]["cwd"] = Value::String("unsaved-directory".into());
        context["launch"]["commandLineProperties"]["feature.null"] = Value::Null;
        host.set_setting("springBeans.launchContext", &context.to_string());
        let out = environment::snapshot_project(&project, &BTreeMap::new(), None);
        assert!(matches!(out.resolve("feature.mode").state, State::Unknown(_)));
        assert!(matches!(out.resolve("feature.null").state, State::Unknown(_)));
        let debug = format!("{:?}", crate::configuration::view(&out, &BTreeMap::new(), ""));
        assert!(!debug.contains("production"));
    }
    #[test]
    fn shared_run_file_wins_duplicate_id_and_env_files_stay_project_scoped() {
        let host = MockHost::install();
        let project = host.invocation().project().unwrap();
        environment::invalidate();
        let contract: Value = serde_json::from_str(CONTRACT).unwrap();
        let case =
            contract["fixtures"].as_array().unwrap().iter().find(|c| c["name"] == "prod-persisted-nonboolean").unwrap();
        let raw = case["projectFiles"][".openidea/run.toml"].as_str().unwrap();
        host.add_file(
            ".openidea/run.toml",
            &raw.replace("module = \"app\"", "module = \"app\"\nenv-files = [\".env\"]"),
        );
        host.add_file(
            ".openidea/run.local.toml",
            &raw.replace("--feature.mode=production", "--feature.mode=wrong-local"),
        );
        host.add_file(".env", "DEMO_NAME=from-env-file\nexport FEATURE_ENABLED=true\nCOMPUTED=${UNAVAILABLE}\n");
        let mut context = case["producerExpected"].clone();
        context["launch"]["envFiles"] = serde_json::json!([".env"]);
        context["launch"]["unresolvedSources"] = serde_json::json!(["envFiles", "implicitJvmOptions"]);
        host.set_setting("springBeans.launchContext", &context.to_string());
        let out = environment::snapshot_project(&project, &BTreeMap::new(), None);
        assert_eq!(out.resolve("feature.mode").state, State::Value("production".into()));
        assert_eq!(out.resolve("demo.name").state, State::Value("from-env-file".into()));
        assert!(matches!(out.resolve("computed").state, State::Unknown(_)));
    }
}
