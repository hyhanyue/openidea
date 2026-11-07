//! Window-scoped, transient Spring analysis facts. This is not a launch environment.
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Default)]
pub struct Contexts {
    entries: BTreeMap<(String, PathBuf), Map<String, Value>>,
}
impl Contexts {
    pub fn set(&mut self, client: &str, root: &Path, facts: Map<String, Value>) {
        self.forget(client);
        self.entries.insert((client.to_owned(), root.to_path_buf()), facts);
    }
    pub fn forget(&mut self, client: &str) {
        self.entries.retain(|(window, _), _| window != client);
    }
    pub fn snapshot(&self, client: Option<&str>, root: Option<&Path>) -> Map<String, Value> {
        let Some(root) = root else { return Map::new() };
        if let Some(client) = client {
            return self.entries.get(&(client.to_owned(), root.to_path_buf())).cloned().unwrap_or_default();
        }
        // Editor providers have no window identity. Conflicting windows are unknown,
        // never an arbitrary window's profile. Views and commands retain their window.
        let mut matching = self.entries.iter().filter(|((_, project), _)| project == root).map(|(_, facts)| facts);
        let Some(first) = matching.next() else { return Map::new() };
        if matching.all(|facts| facts == first) {
            first.clone()
        } else {
            Map::from_iter([("springBeans.launchContext".into(), serde_json::json!({"version":1,"launch":null}))])
        }
    }
}

pub const PLUGINS: &[&str] = &["hyhanyue.spring-beans", "aritti.spring-beans", "hyhanyue.spring-beans-local"];
pub fn is_plugin(id: &str) -> bool {
    PLUGINS.contains(&id)
}
pub const COMMAND: &str = "openidea.setLaunchContext";

pub fn parse_request(raw: &str, root: &Path) -> Result<Map<String, Value>, &'static str> {
    if raw.len() > 64 * 1024 {
        return Err("Spring launch context is too large");
    }
    let request: Value = serde_json::from_str(raw).map_err(|_| "Invalid Spring launch context")?;
    if request.get("projectRoot").and_then(Value::as_str).map(Path::new) != Some(root) {
        return Err("Spring launch context project changed");
    }
    let context = request.get("context").ok_or("Invalid Spring launch context")?;
    parse(&context.to_string())
}

pub fn parse(raw: &str) -> Result<Map<String, Value>, &'static str> {
    if raw.len() > 64 * 1024 {
        return Err("Spring launch context is too large");
    }
    let input: Value = serde_json::from_str(raw).map_err(|_| "Invalid Spring launch context")?;
    if input.is_null() {
        return Ok(Map::new());
    }
    let invalid = "Invalid Spring launch context fact";
    if input.get("version").and_then(Value::as_u64) != Some(1) || input.as_object().is_none_or(|o| o.len() != 2) {
        return Err(invalid);
    }
    if input.get("launch").is_some_and(Value::is_null) {
        return Ok(Map::from_iter([("springBeans.launchContext".into(), input)]));
    }
    let launch = input.get("launch").and_then(Value::as_object).ok_or(invalid)?;
    let keys = [
        "configId",
        "name",
        "kind",
        "persisted",
        "mainClass",
        "module",
        "cwd",
        "profiles",
        "commandLineProperties",
        "systemProperties",
        "environment",
        "inheritEnvironment",
        "envFiles",
        "unresolvedSources",
    ];
    if launch.len() != keys.len() || launch.keys().any(|key| !keys.contains(&key.as_str())) {
        return Err(invalid);
    }
    for key in ["configId", "name", "kind"] {
        if !launch.get(key).and_then(Value::as_str).is_some_and(safe_text) {
            return Err(invalid);
        }
    }
    if !matches!(launch.get("kind").and_then(Value::as_str), Some("application" | "spring-boot" | "maven")) {
        return Err(invalid);
    }
    for key in ["mainClass", "module", "cwd"] {
        if !launch.get(key).is_some_and(|v| v.is_null() || v.as_str().is_some_and(safe_text)) {
            return Err(invalid);
        }
    }
    for key in ["persisted", "inheritEnvironment"] {
        if !launch.get(key).is_some_and(Value::is_boolean) {
            return Err(invalid);
        }
    }
    for key in ["envFiles", "unresolvedSources"] {
        let values = launch.get(key).and_then(Value::as_array).ok_or(invalid)?;
        if values.len() > 128 || values.iter().any(|v| !v.as_str().is_some_and(safe_text)) {
            return Err(invalid);
        }
    }
    let profile = launch.get("profiles").and_then(Value::as_object).ok_or(invalid)?;
    if profile.len() != 3
        || !matches!(profile.get("state").and_then(Value::as_str), Some("known" | "unknown"))
        || !matches!(
            profile.get("source").and_then(Value::as_str),
            Some("commandLine" | "systemProperties" | "environment" | "unknown")
        )
    {
        return Err(invalid);
    }
    let values = profile.get("values").and_then(Value::as_array).ok_or(invalid)?;
    if values.len() > 128
        || values.iter().any(|v| {
            !v.as_str().is_some_and(|v| v.len() <= 128 && v.chars().all(|c| c.is_alphanumeric() || "_.- ".contains(c)))
        })
    {
        return Err(invalid);
    }
    if profile.get("state").and_then(Value::as_str) == Some("unknown") && !values.is_empty() {
        return Err(invalid);
    }
    for key in ["commandLineProperties", "systemProperties", "environment"] {
        let properties = launch.get(key).and_then(Value::as_object).ok_or(invalid)?;
        if properties.len() > 512 {
            return Err(invalid);
        }
        for (key, fact) in properties {
            if key.len() > 256 || !safe_text(key) {
                return Err(invalid);
            }
            let fact = fact.as_object().ok_or(invalid)?;
            match fact.get("state").and_then(Value::as_str) {
                Some("unknown") if fact.len() == 1 => {}
                Some("known")
                    if fact.len() == 2
                        && fact.get("value").and_then(Value::as_str).is_some_and(|v| {
                            matches!(v, "true" | "false")
                                || matches!(key.as_str(), "spring.profiles.active" | "SPRING_PROFILES_ACTIVE")
                                    && v.len() <= 1024
                                    && v.chars().all(|c| c.is_alphanumeric() || "_., -".contains(c))
                        }) => {}
                _ => return Err("Spring launch properties must be redacted or boolean"),
            }
        }
    }
    Ok(Map::from_iter([("springBeans.launchContext".into(), input)]))
}
fn safe_text(text: &str) -> bool {
    text.len() <= 4096 && !text.chars().any(char::is_control)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sample(profile: &str) -> Map<String, Value> {
        let value = serde_json::json!({"version":1,"launch":{
            "configId":format!("user:{profile}"),"name":profile,"kind":"spring-boot","persisted":true,
            "mainClass":"a.App","module":"app","cwd":"work","profiles":{"state":"known","values":[profile],"source":"commandLine"},
            "commandLineProperties":{},"systemProperties":{},"environment":{},"inheritEnvironment":false,"envFiles":[],"unresolvedSources":[]
        }});
        parse(&value.to_string()).unwrap()
    }
    #[test]
    fn rejects_a_delayed_request_from_a_different_project() {
        let request = serde_json::json!({"projectRoot":"/old","context":sample("dev")["springBeans.launchContext"]});
        assert!(parse_request(&request.to_string(), Path::new("/new")).is_err());
        assert!(parse_request(&request.to_string(), Path::new("/old")).is_ok());
        assert!(parse_request(r#"{"projectRoot":"/old","context":null}"#, Path::new("/old")).unwrap().is_empty());
    }
    #[test]
    fn windows_projects_and_invocation_snapshots_are_isolated() {
        let mut contexts = Contexts::default();
        let root = Path::new("/project");
        contexts.set("one", root, sample("dev"));
        let old = contexts.snapshot(Some("one"), Some(root));
        contexts.set("two", root, sample("prod"));
        assert_eq!(contexts.snapshot(Some("one"), Some(root)), sample("dev"));
        assert_eq!(contexts.snapshot(Some("two"), Some(root)), sample("prod"));
        assert!(contexts.snapshot(None, Some(root))["springBeans.launchContext"]["launch"].is_null());
        assert!(contexts.snapshot(Some("one"), Some(Path::new("/other"))).is_empty());
        contexts.set("one", root, parse("null").unwrap());
        assert!(contexts.snapshot(Some("one"), Some(root)).is_empty());
        assert!(contexts.snapshot(None, Some(root))["springBeans.launchContext"]["launch"].is_null());
        assert_eq!(old, sample("dev"));
        contexts.forget("two");
        assert!(contexts.snapshot(None, Some(root)).is_empty());
    }
    #[test]
    fn rejects_raw_properties_without_echoing_values() {
        let mut context = sample("dev")["springBeans.launchContext"].clone();
        context["launch"]["commandLineProperties"] =
            serde_json::json!({"db.password":{"state":"known","value":"synthetic-private-value"}});
        let error = parse(&context.to_string()).unwrap_err();
        assert!(!error.contains("synthetic-private-value"));
        context["launch"]["commandLineProperties"] = serde_json::json!({"feature.mode":{"state":"unknown"}});
        assert!(parse(&context.to_string()).is_ok());
    }
}
