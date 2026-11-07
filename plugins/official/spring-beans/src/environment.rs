//! Bounded Boot 2.7 configuration sources. Values stay in memory; views expose origins, not secrets.
use crate::{FileModel, context};
use openidea_plugin::{Location, Project, host};
use serde_json::Value;
use spring_text::{config, range};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

const INHERITED: u64 = 1;
const ENVFILE: u64 = 2;
const ENV: u64 = 3;
const SYS: u64 = 4;
const JSON: u64 = 5;
const CLI: u64 = 6;
const MANUAL: u64 = 7;
const LIMIT: usize = 128;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Source {
    pub key: String,
    pub raw: Option<String>,
    pub origin: String,
    pub location: Option<Location>,
    pub known: bool,
    priority: Vec<u64>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum State {
    Value(String),
    Missing,
    Unknown(String),
}
#[derive(Clone, Debug)]
pub struct Resolution {
    pub state: State,
    pub chain: Vec<String>,
}
#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub application: String,
    pub profiles: Option<BTreeSet<String>>,
    pub(super) profile_order: Vec<String>,
    pub sources: Vec<Source>,
    pub warnings: BTreeSet<String>,
    pub launch: Option<String>,
    pub notes: BTreeSet<String>,
    pub texts: BTreeMap<String, String>,
    inherited_environment: bool,
    unavailable_layers: BTreeSet<u64>,
    complete: bool,
    loaded: BTreeMap<String, Vec<u64>>,
    loading: BTreeSet<String>,
    resource: String,
    cwd: String,
    base_phase: bool,
}
struct Cache {
    at: Instant,
    signature: String,
    value: Snapshot,
}
thread_local! { static CACHE: RefCell<BTreeMap<String, Cache>> = const { RefCell::new(BTreeMap::new()) }; }
pub fn invalidate() {
    CACHE.with(|cache| cache.borrow_mut().clear());
}
fn profile_list(value: &str) -> Vec<String> {
    value.split(',').map(str::trim).filter(|s| !s.is_empty()).map(str::to_owned).collect()
}
pub(super) fn local(root: &str, base: &str, path: &str) -> Option<String> {
    let value = if path.starts_with('/') { path.to_owned() } else { format!("{base}/{path}") };
    let mut parts = vec![];
    for part in value.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            _ => parts.push(part),
        }
    }
    let path = format!("/{}", parts.join("/"));
    (path == root || path.starts_with(&format!("{root}/"))).then_some(path)
}
fn entries() -> Vec<(String, Option<String>)> {
    let value = host::setting("springBeans.propertyOverrides").and_then(|s| serde_json::from_str::<Value>(&s).ok());
    match value {
        Some(Value::Object(map)) => map.into_iter().map(|(k, v)| (k, v.as_str().map(str::to_owned))).collect(),
        Some(Value::String(text)) => config::properties(&text)
            .into_iter()
            .map(|k| (k.key, Some(k.value)))
            .chain(text.lines().filter_map(|s| s.trim().strip_prefix('!')).map(|s| (s.trim().to_owned(), None)))
            .collect(),
        _ => vec![],
    }
}
fn flatten(prefix: &str, value: &Value, out: &mut Vec<(String, String)>) {
    if let Value::Object(map) = value {
        for (key, value) in map {
            flatten(&if prefix.is_empty() { key.clone() } else { format!("{prefix}.{key}") }, value, out);
        }
    } else if let Value::Array(values) = value {
        for (index, value) in values.iter().enumerate() {
            flatten(&format!("{prefix}[{index}]"), value, out);
        }
    } else if !value.is_null() {
        out.push((prefix.into(), value.as_str().map(str::to_owned).unwrap_or_else(|| value.to_string())));
    }
}

pub fn snapshot(files: &BTreeMap<String, FileModel>, current: Option<(&str, &str)>) -> Snapshot {
    let Some(project) = files.keys().find_map(|p| spring_text::index::project_of(p)) else {
        return Snapshot::default();
    };
    snapshot_project(&project, files, current)
}
pub fn snapshot_project(
    project: &Project,
    files: &BTreeMap<String, FileModel>,
    current: Option<(&str, &str)>,
) -> Snapshot {
    build(project, files, current)
}
fn build(project: &Project, files: &BTreeMap<String, FileModel>, current: Option<(&str, &str)>) -> Snapshot {
    let root = project.root().unwrap_or_default();
    let applications: Vec<_> =
        files.iter().flat_map(|(p, m)| m.context.application_names().map(move |n| (p, n))).collect();
    let signature = format!(
        "{:?}:{:?}:{:?}:{:?}:{:?}:{applications:?}",
        host::setting("springBeans.launchContext"),
        host::setting("springBeans.applicationClass"),
        host::setting("springBeans.activeProfiles"),
        host::setting("springBeans.propertyOverrides"),
        host::setting("springBeans.environmentComplete")
    );
    if current.is_none()
        && let Some(value) = CACHE.with(|c| {
            c.borrow()
                .get(&root)
                .filter(|v| v.signature == signature && v.at.elapsed() < Duration::from_millis(50))
                .map(|v| v.value.clone())
        })
    {
        return value;
    }
    let selected = crate::launch::read(project);
    let application =
        host::setting_string("springBeans.applicationClass").filter(|s| !s.is_empty()).unwrap_or_else(|| {
            if selected.unknown_selection {
                String::new()
            } else if !selected.main.is_empty() {
                selected.main.clone()
            } else if applications.len() == 1 {
                applications[0].1.to_owned()
            } else {
                String::new()
            }
        });
    let module = files
        .iter()
        .find(|(_, m)| m.types.iter().any(|t| t.qualified == application))
        .and_then(|(p, _)| p.split_once("/src/").map(|(m, _)| m.to_owned()))
        .or_else(|| local(&root, &root, &selected.module))
        .unwrap_or_else(|| root.clone());
    let mut seed = Snapshot {
        application,
        launch: selected.name.clone(),
        inherited_environment: selected.inherited,
        warnings: selected.warnings.clone(),
        resource: format!("{module}/src/main/resources"),
        cwd: local(&root, &root, &selected.cwd).unwrap_or_else(|| root.clone()),
        ..Snapshot::default()
    };
    if local(&root, &root, &selected.cwd).is_none() || selected.cwd.contains('$') {
        seed.warnings.insert("Launch working directory is unavailable inside the authorized project".into());
    }
    if local(&root, &root, &selected.module).is_none() || selected.module.contains('$') {
        seed.warnings.insert("Launch module is unavailable inside the authorized project".into());
    }
    if selected.inherited && !host::setting_bool("springBeans.environmentComplete").unwrap_or(false) {
        seed.unavailable_layers.insert(INHERITED);
    }
    if selected.unknown_selection {
        seed.unavailable_layers.insert(CLI);
        seed.warnings.insert("Launch selection is unknown or conflicts across windows".into());
    }
    if selected.unresolved.contains("implicitJvmOptions") {
        seed.unavailable_layers.insert(SYS);
    }
    if selected.unresolved.contains("envFiles") {
        seed.unavailable_layers.insert(ENVFILE);
    }
    let application_json = selected
        .cli
        .get("spring.application.json")
        .or_else(|| selected.system.get("spring.application.json"))
        .or_else(|| selected.env.get("SPRING_APPLICATION_JSON"))
        .or_else(|| selected.env_files.get("SPRING_APPLICATION_JSON"))
        .cloned();
    for (values, layer, origin) in [
        (selected.env_files, ENVFILE, "Launch env file"),
        (selected.env, ENV, "Launch environment"),
        (selected.system, SYS, "JVM system property"),
        (selected.cli, CLI, "Command-line argument"),
    ] {
        for (key, fact) in values {
            seed.sources.push(Source {
                key,
                raw: fact.value().map(str::to_owned),
                origin: origin.into(),
                location: None,
                known: matches!(fact, crate::launch::Fact::Known(_)),
                priority: vec![layer],
            });
        }
    }
    match application_json {
        Some(crate::launch::Fact::Known(raw)) => {
            if let Ok(json) = serde_json::from_str::<Value>(&raw) {
                let mut values = vec![];
                flatten("", &json, &mut values);
                for (key, value) in values {
                    seed.sources.push(Source {
                        key,
                        raw: Some(value),
                        origin: "Spring application JSON".into(),
                        location: None,
                        known: true,
                        priority: vec![JSON],
                    });
                }
            } else {
                seed.unavailable_layers.insert(JSON);
                seed.warnings.insert("Spring application JSON is invalid".into());
            }
        }
        Some(crate::launch::Fact::Unknown) => {
            seed.unavailable_layers.insert(JSON);
        }
        None if selected.unresolved.contains("springApplicationJson") => {
            seed.unavailable_layers.insert(JSON);
        }
        None => {}
    }
    for (key, raw) in entries() {
        seed.sources.push(Source {
            key,
            raw,
            origin: "Analysis override".into(),
            location: None,
            known: true,
            priority: vec![MANUAL],
        });
    }
    // Profiles are deduced from non-profile documents and their imports before any profile variants are loaded.
    let mut base = seed.clone();
    base.base_phase = true;
    base.update_complete();
    base.load_locations(project, current);
    base.update_complete();
    let order = base.accepted_profiles();
    let mut out = seed;
    out.notes.extend(base.notes);
    out.warnings.extend(base.warnings);
    if let Some(order) = order {
        out.profiles = Some(order.iter().cloned().collect());
        out.profile_order = order;
    }
    out.load_locations(project, current);
    out.update_complete();
    if current.is_none() {
        CACHE.with(|cache| {
            let mut cache = cache.borrow_mut();
            if cache.len() >= 8 {
                cache.clear();
            }
            cache.insert(root, Cache { at: Instant::now(), signature, value: out.clone() });
        });
    }
    out
}
// Boot lifts imported profile variants ahead of ordinary profile files
// within the same location group. Imports of profile files retain their enclosing profile order.
fn promote(priority: &mut [u64]) {
    if priority.len() >= 6 && priority[0] == 0 && priority[2] == 0 {
        priority[2] = 2;
    }
}
#[derive(Clone)]
struct LocalLocation {
    path: String,
    optional: bool,
    directory: bool,
}
impl Snapshot {
    fn update_complete(&mut self) {
        self.complete = (!self.inherited_environment
            || host::setting_bool("springBeans.environmentComplete").unwrap_or(false))
            && self.warnings.is_empty()
            && self.unavailable_layers.is_empty();
    }
    fn target(&mut self, project: &Project, raw: &str, parent: Option<&str>) -> Option<LocalLocation> {
        let optional = raw.starts_with("optional:");
        let raw = raw.strip_prefix("optional:").unwrap_or(raw);
        let root = project.root().unwrap_or_default();
        let directory = raw.ends_with('/');
        let (base, raw) = if let Some(raw) = raw.strip_prefix("classpath:") {
            (self.resource.as_str(), raw.trim_start_matches('/'))
        } else if let Some(raw) = raw.strip_prefix("file:") {
            (self.cwd.as_str(), raw)
        } else {
            (parent.unwrap_or(&self.cwd), raw)
        };
        if raw.contains([':', '$', '[', ']', '\\']) || raw.is_empty() && !directory {
            self.warnings.insert("An imported configuration source cannot be evaluated locally".into());
            return None;
        }
        let Some(path) = local(&root, base, raw) else {
            self.warnings.insert("A configuration source is outside the authorized project".into());
            return None;
        };
        Some(LocalLocation { path, optional, directory })
    }
    fn load_locations(&mut self, project: &Project, current: Option<(&str, &str)>) {
        let names = match self.resolve("spring.config.name").state {
            State::Value(value) => profile_list(&value),
            _ => vec!["application".into()],
        };
        if names.iter().any(|n| n.contains(['/', '*', '$', '\\'])) {
            self.warnings.insert("Computed configuration names require runtime confirmation".into());
            return;
        }
        let defaults = "optional:classpath:/;optional:classpath:/config/,optional:file:./;optional:file:./config/;optional:file:./config/*/";
        let locations = match self.resolve("spring.config.location").state {
            State::Value(value) => value,
            _ => defaults.into(),
        };
        let additional = match self.resolve("spring.config.additional-location").state {
            State::Value(value) => format!(",{value}"),
            _ => String::new(),
        };
        let groups = format!("{locations}{additional}");
        let mut tasks = vec![];
        for (group, raw_group) in groups.split(',').enumerate() {
            let mut locations = vec![];
            for raw in raw_group.split(';').map(str::trim).filter(|s| !s.is_empty()) {
                if let Some(location) = self.target(project, raw, None) {
                    if location.path.contains('*') {
                        // Only the final external directory segment may be a wildcard.
                        if raw.contains("classpath:") || !location.directory || !location.path.ends_with("/*") {
                            self.warnings.insert("Unsupported configuration wildcard".into());
                            continue;
                        }
                        let root = project.root().unwrap_or_default();
                        let prefix = location.path.strip_prefix(&format!("{root}/")).unwrap_or("");
                        let found = project.find_files(&format!("{prefix}/*"), LIMIT as u32).unwrap_or_default();
                        if found.len() >= LIMIT {
                            self.warnings.insert("Configuration source limit reached".into());
                        }
                        let parents: BTreeSet<_> = found
                            .iter()
                            .filter_map(|p| p.rsplit_once('/').map(|(p, _)| p.to_owned()))
                            .filter(|p| {
                                p.rsplit_once('/').is_some_and(|(base, _)| format!("{base}/*") == location.path)
                            })
                            .collect();
                        locations.extend(parents.into_iter().map(|path| LocalLocation { path, ..location.clone() }));
                    } else {
                        locations.push(location);
                    }
                }
            }
            let profiles: Vec<Option<&str>> = std::iter::once(None)
                .chain(if self.base_phase {
                    vec![]
                } else {
                    self.profile_order.iter().map(|s| Some(s.as_str())).collect()
                })
                .collect();
            for (profile_order, profile) in profiles.iter().enumerate() {
                for (place, location) in locations.iter().enumerate() {
                    let candidates: Vec<_> = if location.directory {
                        names
                            .iter()
                            .flat_map(|name| {
                                ["yaml", "yml", "properties"].into_iter().enumerate().map(move |(format, ext)| {
                                    let suffix = profile.map_or(String::new(), |p| format!("-{p}"));
                                    (format!("{}/{name}{suffix}.{ext}", location.path), format as u64)
                                })
                            })
                            .collect()
                    } else if let Some(profile) = profile {
                        location
                            .path
                            .rsplit_once('.')
                            .map(|(base, ext)| vec![(format!("{base}-{profile}.{ext}"), 0)])
                            .unwrap_or_default()
                    } else {
                        vec![(location.path.clone(), 0)]
                    };
                    for (path, format) in candidates {
                        tasks.push((
                            path,
                            vec![
                                0,
                                group as u64,
                                u64::from(profile.is_some()),
                                profile_order as u64,
                                place as u64,
                                format,
                            ],
                            profile.is_some(),
                            location.optional || profile.is_some() || location.directory,
                        ));
                    }
                }
            }
            if !self.base_phase && self.profiles.is_none() {
                for (place, location) in locations.iter().enumerate().filter(|(_, l)| l.directory) {
                    for name in &names {
                        let root = project.root().unwrap_or_default();
                        let prefix = location.path.strip_prefix(&format!("{root}/")).unwrap_or("");
                        for (format, ext) in ["yaml", "yml", "properties"].iter().enumerate() {
                            let found = project.find_files(&format!("{prefix}/{name}-*.{ext}"), 32).unwrap_or_default();
                            if found.len() >= 32 {
                                self.warnings.insert("Configuration source limit reached".into());
                            }
                            tasks.extend(found.into_iter().map(|path| {
                                (path, vec![0, group as u64, 1, 1, place as u64, format as u64], true, true)
                            }));
                        }
                    }
                }
            }
        }
        // Highest-precedence declarations claim imports first: Boot imports a resource only once.
        tasks.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| b.0.cmp(&a.0)));
        if let State::Value(imports) = self.resolve("spring.config.import").state {
            let priority = vec![0, u64::MAX];
            self.imports(project, &imports, priority, None, false, current, 0);
        }
        for (path, priority, specific, optional) in tasks {
            let eligible = if specific && self.profiles.is_none() { None } else { Some(true) };
            let found = self.load(project, &path, priority, eligible, specific, current, 0);
            if !found && !optional {
                self.warnings.insert("A required configuration location is unavailable".into());
            }
        }
    }
    #[allow(clippy::too_many_arguments)]
    fn imports(
        &mut self,
        project: &Project,
        raw: &str,
        priority: Vec<u64>,
        parent: Option<&str>,
        specific: bool,
        current: Option<(&str, &str)>,
        depth: usize,
    ) {
        for (index, raw) in raw.split(',').map(str::trim).collect::<Vec<_>>().into_iter().enumerate().rev() {
            let Some(location) = self.target(project, raw, parent) else { continue };
            if location.directory
                || location.path.contains('*')
                || ![".properties", ".yaml", ".yml"].iter().any(|e| location.path.ends_with(e))
            {
                self.warnings.insert("This configuration import format requires runtime confirmation".into());
                continue;
            }
            let mut priority = priority.clone();
            priority.push(index as u64 + 1);
            let mut variants = vec![(location.path.clone(), 0, specific)];
            if !self.base_phase
                && let Some((base, ext)) = location.path.rsplit_once('.')
            {
                variants.extend(
                    self.profile_order
                        .iter()
                        .enumerate()
                        .map(|(i, p)| (format!("{base}-{p}.{ext}"), i as u64 + 1, true)),
                );
            }
            let mut found = false;
            for (path, order, variant_specific) in variants.into_iter().rev() {
                let mut priority = priority.clone();
                if order > 0 {
                    promote(&mut priority);
                }
                priority.push(order);
                found |= self.load(project, &path, priority, Some(true), variant_specific, current, depth + 1);
            }
            if !found {
                if location.optional {
                    self.notes.insert("An optional local import is unavailable".into());
                } else if !matches!(self.resolve("spring.config.on-not-found").state, State::Value(v) if v == "ignore")
                {
                    self.warnings.insert("A required configuration import is unavailable".into());
                }
            }
        }
    }
    #[allow(clippy::too_many_arguments)]
    fn load(
        &mut self,
        project: &Project,
        path: &str,
        priority: Vec<u64>,
        eligible: Option<bool>,
        specific: bool,
        current: Option<(&str, &str)>,
        depth: usize,
    ) -> bool {
        if self.loading.contains(path) {
            self.notes.insert("A configuration import cycle was skipped (resources load once)".into());
            return true;
        }
        if let Some(previous) = self.loaded.get(path) {
            if previous < &priority {
                self.warnings.insert("An import overlaps another location with different precedence; source ordering requires runtime confirmation".into());
            }
            return true;
        }
        if self.texts.len() >= LIMIT || depth > 8 || self.sources.len() >= 8192 {
            self.warnings.insert("Configuration source limit reached".into());
            return false;
        }
        let Some(text) =
            current.filter(|(p, _)| *p == path).map(|(_, t)| t.to_owned()).or_else(|| project.read_file(path).ok())
        else {
            return false;
        };
        if text.len() > 1024 * 1024
            || self.texts.values().map(String::len).sum::<usize>() + text.len() > 8 * 1024 * 1024
        {
            self.warnings.insert("Configuration text limit reached".into());
            return false;
        }
        if ![".properties", ".yaml", ".yml"].iter().any(|e| path.ends_with(e)) {
            self.warnings.insert("This configuration source format requires runtime confirmation".into());
            return false;
        }
        self.loaded.insert(path.into(), priority.clone());
        self.loading.insert(path.into());
        self.texts.insert(path.into(), text.clone());
        for (document, (start, end)) in documents(path, &text).into_iter().enumerate().rev() {
            let keys = config::keys(path, &text[start..end]);
            let activation = keys.iter().find(|k| k.key == "spring.config.activate.on-profile");
            if (specific || activation.is_some())
                && keys.iter().any(|k| {
                    k.key == "spring.profiles.active"
                        || k.key.starts_with("spring.profiles.active[")
                        || k.key == "spring.profiles.include"
                        || k.key.starts_with("spring.profiles.include[")
                        || k.key == "spring.profiles.default"
                        || k.key.starts_with("spring.profiles.group.")
                })
            {
                self.warnings.insert(
                    "Profile activation properties in a profile-specific document are invalid in Boot 2.7".into(),
                );
            }
            if self.base_phase && activation.is_some() {
                continue;
            }
            let state = activation.map_or(eligible, |key| {
                match (eligible, self.profiles.as_ref().and_then(|p| context::profile(&key.value, p))) {
                    (Some(false), _) | (_, Some(false)) => Some(false),
                    (Some(true), Some(true)) => Some(true),
                    _ => None,
                }
            });
            if state == Some(false) {
                continue;
            }
            let other_activation = keys.iter().any(|k| {
                k.key == "spring.config.activate.on-cloud-platform"
                    || k.key == "spring.profiles" && !k.value.is_empty()
                    || k.key.starts_with("spring.profiles[")
            });
            if other_activation {
                self.warnings
                    .insert("Cloud-platform or legacy document activation requires runtime confirmation".into());
            }
            if keys.iter().any(|k| text[start + k.at..start + k.value_end].contains('\\')) {
                self.warnings.insert("Escaped or continued configuration syntax requires runtime confirmation".into());
            }
            let mut doc_priority = priority.clone();
            doc_priority.push(document as u64);
            let mut document_keys = BTreeSet::new();
            for key in &keys {
                if self.sources.len() >= 8192 {
                    self.warnings.insert("Configuration source limit reached".into());
                    break;
                }
                if key.value.is_empty()
                    && keys.iter().any(|child| {
                        child.key.starts_with(&format!("{}.", key.key))
                            || child.key.starts_with(&format!("{}[", key.key))
                    })
                {
                    continue;
                }
                let raw = &text[start + key.value_at..start + key.value_end];
                let yaml = !path.ends_with(".properties");
                let quote = (yaml && start + key.value_at > 0)
                    .then(|| text.as_bytes()[start + key.value_at - 1])
                    .filter(|c| b"'\"".contains(c));
                let complex = yaml
                    && (quote.is_none() && raw.starts_with(['&', '*', '!', '{', '[', '|', '>'])
                        || quote.is_some()
                            && text[start + key.value_end..]
                                .split('\n')
                                .next()
                                .unwrap_or("")
                                .get(1..)
                                .unwrap_or("")
                                .trim_start()
                                .chars()
                                .next()
                                .is_some_and(|c| c != '#'));
                if complex || yaml && !document_keys.insert(key.key.clone()) {
                    self.warnings.insert("Complex or duplicate YAML syntax requires runtime confirmation".into());
                }
                let mut value = key.value.clone();
                if yaml && quote.is_none() {
                    value = match value.to_ascii_lowercase().as_str() {
                        "null" | "~" => String::new(),
                        "true" | "yes" | "on" => "true".into(),
                        "false" | "no" | "off" => "false".into(),
                        _ => value,
                    };
                }
                let number = yaml
                    && quote.is_none()
                    && raw.chars().next().is_some_and(|c| c.is_ascii_digit() || ['+', '-', '.'].contains(&c));
                let numeric_unknown = number
                    && !raw.parse::<i64>().is_ok_and(|n| n.to_string() == raw)
                    && !raw.parse::<f64>().is_ok_and(|n| n.is_finite() && n.to_string() == raw);
                let known = state == Some(true)
                    && !other_activation
                    && !complex
                    && !numeric_unknown
                    && (!path.ends_with(".properties") || raw.is_ascii() && key.key.is_ascii())
                    && !raw.contains('\\')
                    && !key.key.contains('\\');
                self.sources.push(Source {
                    key: key.key.clone(),
                    raw: Some(value),
                    origin: format!("{} · document {}", path.rsplit('/').next().unwrap_or(path), document + 1),
                    location: Some(Location {
                        path: path.into(),
                        range: range(&text, start + key.at, start + key.end),
                    }),
                    known,
                    priority: doc_priority.clone(),
                });
            }
            if state != Some(true) && keys.iter().any(|k| k.key == "spring.config.import") {
                self.warnings.insert("A configuration import has undetermined document activation".into());
                continue;
            }
            for imports in keys.iter().filter(|k| k.key == "spring.config.import") {
                let parent = path.rsplit_once('/').map(|(p, _)| p);
                self.imports(
                    project,
                    &imports.value,
                    doc_priority.clone(),
                    parent,
                    specific || activation.is_some(),
                    current,
                    depth,
                );
            }
        }
        self.loading.remove(path);
        true
    }
    fn list_from(&self, key: &str, priority: Option<&[u64]>) -> Result<Option<Vec<String>>, ()> {
        let mut groups: BTreeMap<Vec<u64>, Vec<(String, &Source)>> = BTreeMap::new();
        let variable = key.replace('.', "_").replace('-', "").to_uppercase();
        for source in &self.sources {
            if priority.is_some_and(|p| source.priority != p) {
                continue;
            }
            let name = if [ENV, ENVFILE].contains(source.priority.first().unwrap_or(&0)) {
                if source.key == variable {
                    key.to_owned()
                } else if let Some(index) = source
                    .key
                    .strip_prefix(&format!("{variable}_"))
                    .and_then(|s| s.strip_suffix('_'))
                    .and_then(|s| s.parse::<usize>().ok())
                {
                    format!("{key}[{index}]")
                } else {
                    continue;
                }
            } else {
                source.key.clone()
            };
            if name == key || name.starts_with(&format!("{key}[")) {
                groups.entry(source.priority.clone()).or_default().push((name, source));
            }
        }
        let Some((_, sources)) = groups.last_key_value() else { return Ok(None) };
        if sources.iter().any(|(_, s)| {
            !s.known
                || s.priority.first() == Some(&0) && !self.complete
                || self.unavailable_layers.iter().any(|layer| s.priority.first().is_some_and(|p| p <= layer))
        }) {
            return Err(());
        }
        if let Some((_, source)) = sources.iter().rev().find(|(name, _)| name == key) {
            let Some(raw) = &source.raw else { return Ok(Some(vec![])) };
            return match self.resolve_text(raw, &mut BTreeSet::new(), &mut vec![], 0) {
                State::Value(value) => Ok(Some(profile_list(&value))),
                _ => Err(()),
            };
        }
        let mut indexed = BTreeMap::new();
        for (name, source) in sources {
            let Some(index) = name
                .strip_prefix(&format!("{key}["))
                .and_then(|s| s.strip_suffix(']'))
                .and_then(|s| s.parse::<usize>().ok())
            else {
                return Err(());
            };
            let Some(raw) = &source.raw else { return Err(()) };
            let State::Value(value) = self.resolve_text(raw, &mut BTreeSet::new(), &mut vec![], 0) else {
                return Err(());
            };
            indexed.insert(index, value);
        }
        if indexed.keys().copied().ne(0..indexed.len()) {
            return Err(());
        }
        Ok(Some(indexed.into_values().collect()))
    }
    fn accepted_profiles(&mut self) -> Option<Vec<String>> {
        if !self.warnings.is_empty() {
            return None;
        }
        let explicit = host::setting_string("springBeans.activeProfiles").filter(|value| !value.trim().is_empty());
        let mut active = match explicit {
            Some(value) => profile_list(&value),
            None => match self.list_from("spring.profiles.active", None) {
                Ok(Some(value)) => value,
                Ok(None) if self.complete => vec![],
                _ => return None,
            },
        };
        let mut included = vec![];
        let priorities: BTreeSet<_> = self.sources.iter().map(|s| s.priority.clone()).collect();
        for priority in priorities.iter().rev() {
            match self.list_from("spring.profiles.include", Some(priority)) {
                Ok(Some(values)) => included.extend(values),
                Ok(None) => {}
                Err(()) => return None,
            }
        }
        included.append(&mut active);
        if included.is_empty() {
            included = self.list_from("spring.profiles.default", None).ok()?.unwrap_or_else(|| vec!["default".into()]);
        }
        let mut order = vec![];
        let mut pending: Vec<_> = included.into_iter().rev().map(|p| (p, Vec::<String>::new())).collect();
        while let Some((profile, mut parents)) = pending.pop() {
            if parents.contains(&profile) {
                self.notes.insert("A profile group cycle was stopped after its first expansion".into());
                continue;
            }
            if order.contains(&profile) {
                continue;
            }
            if profile.contains(['!', '&', '|', '(', ')', '$']) || order.len() >= 64 {
                self.warnings.insert("Profile expansion is invalid or exceeds the analysis limit".into());
                return None;
            }
            order.push(profile.clone());
            let children = self.list_from(&format!("spring.profiles.group.{profile}"), None).ok()?.unwrap_or_default();
            parents.push(profile);
            pending.extend(children.into_iter().rev().map(|p| (p, parents.clone())));
        }
        Some(order)
    }
    pub fn keys(&self) -> BTreeSet<String> {
        self.sources
            .iter()
            .filter(|s| ![ENV, ENVFILE].contains(s.priority.first().unwrap_or(&0)))
            .map(|s| s.key.clone())
            .collect()
    }
    pub fn candidates(&self, key: &str) -> Vec<&Source> {
        let variable = key
            .replace("].", "_")
            .replace("][", "_")
            .replace('.', "_")
            .replace('-', "")
            .replace(['[', ']'], "_")
            .to_uppercase();
        let mut values: Vec<_> = self
            .sources
            .iter()
            .enumerate()
            .filter(|(_, s)| {
                s.key == key || [ENV, ENVFILE].contains(s.priority.first().unwrap_or(&0)) && s.key == variable
            })
            .collect();
        values.sort_by(|(i, a), (j, b)| a.priority.cmp(&b.priority).then(i.cmp(j)));
        values.into_iter().map(|(_, s)| s).collect()
    }
    pub fn resolve(&self, key: &str) -> Resolution {
        let mut chain = vec![];
        let mut seen = BTreeSet::new();
        let state = self.resolve_key(key, &mut seen, &mut chain, 0);
        Resolution { state, chain }
    }
    fn resolve_key(&self, key: &str, seen: &mut BTreeSet<String>, chain: &mut Vec<String>, depth: usize) -> State {
        chain.push(key.into());
        if depth > 32 || !seen.insert(key.into()) {
            return State::Unknown("Placeholder cycle or depth limit".into());
        }
        let result = match self.candidates(key).last() {
            Some(source)
                if self.unavailable_layers.iter().any(|layer| source.priority.first().is_some_and(|p| p <= layer)) =>
            {
                State::Unknown("A higher-priority launch source is unavailable".into())
            }
            Some(source) if !source.known => State::Unknown("Profile or source syntax is not determined".into()),
            Some(source) if source.priority.first() == Some(&0) && !self.warnings.is_empty() => {
                State::Unknown("A configuration source cannot be evaluated".into())
            }
            Some(source)
                if source.priority.first() == Some(&0)
                    && self.inherited_environment
                    && !host::setting_bool("springBeans.environmentComplete").unwrap_or(false) =>
            {
                State::Unknown("Inherited process environment may override this source".into())
            }
            Some(source) => match &source.raw {
                Some(value) => self.resolve_text(value, seen, chain, depth + 1),
                None => State::Missing,
            },
            None if self.complete => State::Missing,
            None => State::Unknown("Property may come from an unavailable environment or source".into()),
        };
        seen.remove(key);
        result
    }
    fn resolve_text(&self, text: &str, seen: &mut BTreeSet<String>, chain: &mut Vec<String>, depth: usize) -> State {
        if text.contains("#{") {
            return State::Unknown("SpEL is evaluated only by the application".into());
        }
        let mut output = String::new();
        let mut at = 0;
        for placeholder in placeholders(text) {
            output.push_str(&text[at..placeholder.from]);
            match self.resolve_key(&placeholder.key, seen, chain, depth + 1) {
                State::Value(value) => output.push_str(&value),
                State::Missing => {
                    if let Some(default) = placeholder.default {
                        match self.resolve_text(&default, seen, chain, depth + 1) {
                            State::Value(v) => output.push_str(&v),
                            other => return other,
                        }
                    } else {
                        return State::Unknown("Unresolved required placeholder".into());
                    }
                }
                other => return other,
            }
            at = placeholder.end;
        }
        if text[at..].contains("${") {
            return State::Unknown("Computed or incomplete placeholder".into());
        }
        output.push_str(&text[at..]);
        State::Value(output)
    }
}
pub struct Placeholder {
    pub from: usize,
    pub key_at: usize,
    pub key: String,
    pub default: Option<String>,
    pub default_at: Option<usize>,
    pub end: usize,
}
pub fn placeholders(text: &str) -> Vec<Placeholder> {
    let mut out = vec![];
    let mut at = 0;
    while let Some(found) = text[at..].find("${") {
        let start = at + found;
        let mut i = start + 2;
        let mut depth = 1;
        let mut colon = None;
        while i < text.len() {
            if text[i..].starts_with("${") {
                depth += 1;
                i += 2;
                continue;
            }
            let c = text.as_bytes()[i];
            if c == b'}' {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            } else if c == b':' && depth == 1 && colon.is_none() {
                colon = Some(i);
            }
            i += text[i..].chars().next().map_or(1, char::len_utf8);
        }
        if depth != 0 {
            break;
        }
        let key_end = colon.unwrap_or(i);
        let key = text[start + 2..key_end].trim();
        if !key.is_empty() && !key.contains("${") {
            out.push(Placeholder {
                from: start,
                key_at: start + 2 + text[start + 2..key_end].len() - text[start + 2..key_end].trim_start().len(),
                key: key.into(),
                default: colon.map(|c| text[c + 1..i].into()),
                default_at: colon.map(|c| c + 1),
                end: i + 1,
            });
        }
        at = i + 1;
    }
    out
}
pub fn references(text: &str) -> Vec<(String, usize, usize)> {
    fn add(text: &str, base: usize, depth: usize, out: &mut Vec<(String, usize, usize)>) {
        if depth > 32 {
            return;
        }
        for placeholder in placeholders(text) {
            out.push((
                placeholder.key.clone(),
                base + placeholder.key_at,
                base + placeholder.key_at + placeholder.key.len(),
            ));
            if let (Some(default), Some(at)) = (placeholder.default, placeholder.default_at) {
                add(&default, base + at, depth + 1, out);
            }
        }
    }
    let mut out = vec![];
    add(text, 0, 0, &mut out);
    out
}

fn documents(path: &str, text: &str) -> Vec<(usize, usize)> {
    let mut out = vec![];
    let mut start = 0;
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if if path.ends_with(".properties") {
            trimmed == "#---" || trimmed == "!---"
        } else {
            trimmed == "---" || trimmed.starts_with("--- ")
        } {
            out.push((start, offset));
            start = offset + line.len();
        }
        offset += line.len();
    }
    out.push((start, text.len()));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;
    fn envelope() -> Option<Value> {
        host::setting("springBeans.launchContext").and_then(|s| serde_json::from_str(&s).ok())
    }
    fn setup(profiles: &str) -> (MockHost, Project) {
        invalidate();
        let host = MockHost::install();
        let value = serde_json::json!({"version":1,"launch":{
            "name":"Local fixture","mainClass":"example.App","profiles":{"state":"known","values":profiles.split(',').collect::<Vec<_>>(),"source":"commandLine"},
            "inheritEnvironment":false,"environment":{"DEMO_NAME":{"state":"known","value":"environment"}},"systemProperties":{"demo.name":{"state":"known","value":"system"}},"commandLineProperties":{"spring.profiles.active":{"state":"known","value":profiles}}
        }});
        host.set_setting("springBeans.launchContext", &value.to_string());
        let project = host.invocation().project().unwrap();
        (host, project)
    }
    #[test]
    fn launch_and_external_config_precedence_and_placeholder_chain() {
        let (host, project) = setup("dev");
        host.add_file(
            "src/main/resources/application.properties",
            "demo.name=base\ndemo.greeting=hello ${demo.alias}\ndemo.alias=${demo.name}\n",
        );
        host.add_file("src/main/resources/application-dev.yml", "demo:\n  name: profile\n");
        host.add_file("config/application.properties", "demo.name=external\n");
        let snapshot = snapshot_project(&project, &BTreeMap::new(), None);
        assert_eq!(snapshot.resolve("demo.greeting").state, State::Value("hello system".into()));
        assert_eq!(snapshot.resolve("demo.greeting").chain, ["demo.greeting", "demo.alias", "demo.name"]);
        assert_eq!(snapshot.candidates("demo.name").last().unwrap().origin, "JVM system property");
        host.set_setting("springBeans.propertyOverrides", &serde_json::to_string("demo.name=analysis").unwrap());
        let snapshot = snapshot_project(&project, &BTreeMap::new(), None);
        assert_eq!(snapshot.resolve("demo.greeting").state, State::Value("hello analysis".into()));
    }
    #[test]
    fn profile_file_order_and_multidocument_activation() {
        let (host, project) = setup("prod,dev");
        host.add_file("src/main/resources/application-prod.properties", "label=prod\n");
        host.add_file("src/main/resources/application-dev.properties", "label=dev\n");
        host.add_file("src/main/resources/application.yml","section: base\n---\nspring:\n  config:\n    activate:\n      on-profile: prod & !dev\nsection: wrong\n---\nspring.config.activate.on-profile: dev\nsection: right\n");
        let snapshot = snapshot_project(&project, &BTreeMap::new(), None);
        assert_eq!(snapshot.resolve("label").state, State::Value("dev".into()));
        assert_eq!(snapshot.resolve("section").state, State::Value("right".into()));
    }
    #[test]
    fn nested_defaults_cycles_and_unicode_do_not_guess() {
        let (host, project) = setup("dev");
        host.add_file(
            "src/main/resources/application.properties",
            "name=world\nvalue=${missing:${name}}\na=${b}\nb=${a}\n",
        );
        host.add_file("src/main/resources/application-dev.yml", "unicode: \"${名字:默认}\"\n");
        let snapshot = snapshot_project(&project, &BTreeMap::new(), None);
        assert_eq!(snapshot.resolve("value").state, State::Value("world".into()));
        assert_eq!(snapshot.resolve("unicode").state, State::Value("默认".into()));
        assert!(matches!(snapshot.resolve("a").state, State::Unknown(_)));
    }
    #[test]
    fn inherited_environment_remains_unknown_unless_explicitly_overridden() {
        invalidate();
        let host = MockHost::install();
        let project = host.invocation().project().unwrap();
        host.add_file("src/main/resources/application.properties", "flag=true\n");
        let snapshot = snapshot_project(&project, &BTreeMap::new(), None);
        assert!(matches!(snapshot.resolve("flag").state, State::Unknown(_)));
        host.set_setting("springBeans.propertyOverrides", &serde_json::to_string("flag=false\n!absent").unwrap());
        let snapshot = snapshot_project(&project, &BTreeMap::new(), None);
        assert_eq!(snapshot.resolve("flag").state, State::Value("false".into()));
        assert_eq!(snapshot.resolve("absent").state, State::Missing);
    }
    #[test]
    fn unsupported_imports_and_syntax_are_unknown() {
        let (host, project) = setup("dev");
        host.add_file(
            "src/main/resources/application.properties",
            "flag=true\nspring.config.import=https://not-fetched.invalid/config\n",
        );
        let snapshot = snapshot_project(&project, &BTreeMap::new(), None);
        assert!(matches!(snapshot.resolve("flag").state, State::Unknown(_)));
        assert!(host.requests().is_empty());
    }
    #[test]
    fn local_imports_optional_cycles_and_profile_variants_match_boot_order() {
        let (host, project) = setup("dev");
        host.add_file("src/main/resources/application.properties","spring.config.import=classpath:shared/base.properties,file:./external.properties,optional:missing.properties\nflag=application\n");
        host.add_file(
            "src/main/resources/shared/base.properties",
            "flag=shared\nonly.shared=yes\nspring.config.import=../application.properties\n",
        );
        host.add_file("src/main/resources/shared/base-dev.properties", "flag=shared-dev\n");
        host.add_file("external.properties", "flag=external\nspring.config.import=parts/nested.yml\n");
        host.add_file("parts/nested.yml", "nested: resolved\n");
        let out = snapshot_project(&project, &BTreeMap::new(), None);
        assert_eq!(out.resolve("flag").state, State::Value("shared-dev".into()));
        assert_eq!(out.resolve("only.shared").state, State::Value("yes".into()));
        assert_eq!(out.resolve("nested").state, State::Value("resolved".into()));
        assert!(out.notes.iter().any(|n| n.contains("cycle")));
        assert!(out.notes.iter().any(|n| n.contains("optional")));
        assert!(out.warnings.is_empty(), "{:?}", out.warnings);
        assert_eq!(
            out.candidates("flag")
                .iter()
                .filter(|s| s.location.as_ref().is_some_and(|l| l.path.ends_with("shared/base.properties")))
                .count(),
            1
        );
    }
    #[test]
    fn includes_groups_and_indexed_lists_expand_once_in_boot_order() {
        let (host, project) = setup("production");
        host.add_file("src/main/resources/application.yml","spring:\n  profiles:\n    include:\n      - common\n    group:\n      production:\n        - proddb\n        - prodmq\n      proddb: nested\n      nested: production\nflag: base\n");
        for (profile, value) in
            [("common", "common"), ("production", "prod"), ("proddb", "db"), ("nested", "nested"), ("prodmq", "mq")]
        {
            host.add_file(&format!("src/main/resources/application-{profile}.properties"), &format!("flag={value}\n"));
        }
        let out = snapshot_project(&project, &BTreeMap::new(), None);
        assert_eq!(out.profile_order, ["common", "production", "proddb", "nested", "prodmq"]);
        assert_eq!(out.resolve("flag").state, State::Value("mq".into()));
        assert!(out.notes.iter().any(|s| s.contains("profile group cycle")));
        assert!(out.warnings.is_empty());
    }
    #[test]
    fn include_lists_accumulate_by_source_and_group_lists_use_highest_source() {
        let (host, project) = setup("production");
        host.add_file(
            "src/main/resources/application.properties",
            "spring.profiles.include=baseinclude\nspring.profiles.group.production=old1,old2\n",
        );
        host.add_file(
            "config/application.yml",
            "spring.profiles.include:\n  - externalinclude\nspring.profiles.group.production:\n  - override\n",
        );
        let out = snapshot_project(&project, &BTreeMap::new(), None);
        assert_eq!(out.profile_order, ["externalinclude", "baseinclude", "production", "override"]);
    }
    #[test]
    fn default_profiles_and_environment_profile_lists_are_supported() {
        let (host, project) = setup("");
        let mut data = envelope().unwrap();
        data["launch"]["commandLineProperties"].as_object_mut().unwrap().remove("spring.profiles.active");
        data["launch"]["environment"] = serde_json::json!({"SPRING_PROFILES_ACTIVE":{"state":"known","value":"dev"},"SPRING_PROFILES_INCLUDE_0_":{"state":"known","value":"common"}});
        host.set_setting("springBeans.launchContext", &data.to_string());
        let out = snapshot_project(&project, &BTreeMap::new(), None);
        assert_eq!(out.profile_order, ["common", "dev"]);
        data["launch"]["environment"] = serde_json::json!({});
        host.set_setting("springBeans.launchContext", &data.to_string());
        host.add_file("src/main/resources/application.properties", "spring.profiles.default=local\n");
        host.add_file("src/main/resources/application-local.properties", "flag=local\n");
        let out = snapshot_project(&project, &BTreeMap::new(), None);
        assert_eq!(out.profile_order, ["local"]);
        assert_eq!(out.resolve("flag").state, State::Value("local".into()));
    }
    #[test]
    fn profile_specific_activation_properties_are_invalid_instead_of_guessed() {
        let (host, project) = setup("dev");
        host.add_file("src/main/resources/application-dev.properties", "flag=dev\nspring.profiles.include=prod\n");
        let out = snapshot_project(&project, &BTreeMap::new(), None);
        assert!(out.warnings.iter().any(|w| w.contains("invalid in Boot 2.7")));
        assert!(matches!(out.resolve("flag").state, State::Unknown(_)));
    }
    #[test]
    fn required_missing_import_and_outside_project_paths_are_unknown() {
        let (host, project) = setup("dev");
        host.add_file(
            "src/main/resources/application.properties",
            "flag=true\nspring.config.import=file:/etc/secret.properties,missing.properties\n",
        );
        let out = snapshot_project(&project, &BTreeMap::new(), None);
        assert!(out.warnings.iter().any(|w| w.contains("outside the authorized project")));
        assert!(out.warnings.iter().any(|w| w.contains("required configuration import")));
        assert!(matches!(out.resolve("flag").state, State::Unknown(_)));
        assert!(!out.texts.keys().any(|p| p.starts_with("/etc")));
    }
    #[test]
    fn grouped_locations_put_profile_specific_sources_above_other_base_files() {
        let (host, project) = setup("dev");
        host.add_file("src/main/resources/config/application.properties", "flag=classpath-config-base\n");
        host.add_file("src/main/resources/application-dev.properties", "flag=classpath-profile\n");
        host.add_file("config/application.properties", "external=base\n");
        host.add_file("application-dev.properties", "external=profile\n");
        let out = snapshot_project(&project, &BTreeMap::new(), None);
        assert_eq!(out.resolve("flag").state, State::Value("classpath-profile".into()));
        assert_eq!(out.resolve("external").state, State::Value("profile".into()));
    }
    #[test]
    fn later_documents_override_earlier_imports_and_current_import_text_is_used() {
        let (host, project) = setup("dev");
        host.add_file("src/main/resources/application.properties","spring.config.import=parts/shared.properties\nflag=base\n#---\nspring.config.activate.on-profile=dev\nflag=last\n");
        host.add_file("src/main/resources/parts/shared.properties", "flag=imported\nother=on-disk\n");
        let out = snapshot_project(
            &project,
            &BTreeMap::new(),
            Some(("/project/src/main/resources/parts/shared.properties", "flag=unsaved\nother=unsaved\n")),
        );
        assert_eq!(out.resolve("flag").state, State::Value("last".into()));
        assert_eq!(out.resolve("other").state, State::Value("unsaved".into()));
    }
    #[test]
    fn environment_system_json_cli_and_analysis_layers_have_stable_precedence() {
        let (host, project) = setup("dev");
        host.add_file("src/main/resources/application.properties", "demo.name=file\n");
        let mut data = envelope().unwrap();
        data["launch"]["environment"]["SPRING_APPLICATION_JSON"] =
            serde_json::json!({"state":"known","value":r#"{"demo":{"name":"json","items":["a","b"]}}"#});
        host.set_setting("springBeans.launchContext", &data.to_string());
        let out = snapshot_project(&project, &BTreeMap::new(), None);
        assert_eq!(out.resolve("demo.name").state, State::Value("json".into()));
        assert_eq!(out.resolve("demo.items[1]").state, State::Value("b".into()));
        data["launch"]["commandLineProperties"] = serde_json::json!({"demo.name":{"state":"known","value":"cli"}});
        host.set_setting("springBeans.launchContext", &data.to_string());
        assert_eq!(
            snapshot_project(&project, &BTreeMap::new(), None).resolve("demo.name").state,
            State::Value("cli".into())
        );
    }
    #[test]
    fn yaml_boolean_and_null_scalars_keep_quoted_literals_distinct() {
        let (host, project) = setup("dev");
        host.add_file(
            "src/main/resources/application.yml",
            "yes-flag: yes\nno-flag: off\nnull-flag: null\nquoted: 'yes'\n",
        );
        let out = snapshot_project(&project, &BTreeMap::new(), None);
        for (key, value) in [("yes-flag", "true"), ("no-flag", "false"), ("null-flag", ""), ("quoted", "yes")] {
            assert_eq!(out.resolve(key).state, State::Value(value.into()));
        }
    }
    #[test]
    fn complex_yaml_cannot_turn_unseen_keys_into_known_absence() {
        let (host, project) = setup("dev");
        host.add_file(
            "src/main/resources/application.yml",
            "defaults: &base\n  hidden: yes\nmerged:\n  <<: *base\nvalue: ${merged.hidden:fallback}\n",
        );
        let out = snapshot_project(&project, &BTreeMap::new(), None);
        assert!(matches!(out.resolve("value").state, State::Unknown(_)));
        assert!(out.warnings.iter().any(|w| w.contains("YAML")));
    }
    #[test]
    fn indexed_environment_names_keep_one_separator_before_nested_fields() {
        let (host, project) = setup("dev");
        let mut data = envelope().unwrap();
        data["launch"]["environment"]["DEMO_ITEMS_0_NAME"] = serde_json::json!({"state":"known","value":"nested-env"});
        host.set_setting("springBeans.launchContext", &data.to_string());
        let out = snapshot_project(&project, &BTreeMap::new(), None);
        assert_eq!(out.resolve("demo.items[0].name").state, State::Value("nested-env".into()));
    }
}
