//! Eligibility in a selected source context, never a substitute for the runtime.
use crate::model::{Bean, FileModel};
use openidea_plugin::{Range, host};
use serde_json::Value;
use spring_text::java::{self, Annotation, JavaFile, MemberKind};
use spring_text::range;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Active,
    Inactive,
    Unknown,
}
impl State {
    fn and(self, other: Self) -> Self {
        if self == Self::Inactive || other == Self::Inactive {
            Self::Inactive
        } else if self == Self::Unknown || other == Self::Unknown {
            Self::Unknown
        } else {
            Self::Active
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Metadata {
    classes: Vec<Class>,
    beans: Vec<(Range, String, Vec<Condition>)>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Class {
    name: String,
    package: String,
    boot: bool,
    auto: bool,
    scans: Vec<String>,
    imports: Vec<String>,
    uncertain_scan: bool,
    conditions: Vec<Condition>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
enum Condition {
    Profile(Vec<String>),
    Property { names: Vec<String>, having: String, missing: bool },
    Missing { types: Vec<String>, names: Vec<String> },
    Unknown,
}

fn strings(a: &Annotation, keys: &[&str]) -> Vec<String> {
    a.strings(keys).into_iter().map(|s| s.value.clone()).collect()
}
fn classes(a: &Annotation, keys: &[&str]) -> Vec<String> {
    let mut out = Vec::new();
    for arg in &a.args {
        if !keys.contains(&arg.name.as_str()) {
            continue;
        }
        let ts = java::tokens(&arg.text);
        for (i, t) in ts.iter().enumerate() {
            if t.s == "class" && i >= 2 && ts[i - 1].s == "." {
                let mut start = i - 2;
                while start >= 2 && ts[start - 1].s == "." && ts[start - 2].is_ident() {
                    start -= 2;
                }
                out.push(ts[start..i - 1].iter().map(|t| t.s).collect());
            }
        }
    }
    out
}
fn qualify(file: &JavaFile, name: &str) -> String {
    if name.contains('.') {
        return name.to_owned();
    }
    file.imports
        .iter()
        .find(|i| !i.is_static && i.path.ends_with(&format!(".{name}")))
        .map(|i| i.path.clone())
        .unwrap_or_else(|| if file.package.is_empty() { name.into() } else { format!("{}.{name}", file.package) })
}
fn literal_strings(annotation: &Annotation, keys: &[&str]) -> bool {
    annotation
        .args
        .iter()
        .filter(|a| keys.contains(&a.name.as_str()))
        .all(|a| java::tokens(&a.text).iter().all(|t| t.string().is_some() || ["{", "}", ","].contains(&t.s)))
}
fn conditions(file: &JavaFile, annotations: &[Annotation]) -> Vec<Condition> {
    annotations
        .iter()
        .filter(|a| file.from_spring(&a.name))
        .filter_map(|a| {
            Some(match a.name.as_str() {
                "Profile" => {
                    let values = strings(a, &["value"]);
                    if values.is_empty() || !literal_strings(a, &["value"]) {
                        Condition::Unknown
                    } else {
                        Condition::Profile(values)
                    }
                }
                "ConditionalOnProperty" => {
                    let names = strings(a, &["name", "value"]);
                    let prefix = strings(a, &["prefix"]).pop().unwrap_or_default();
                    let having = strings(a, &["havingValue"]).pop().unwrap_or_default();
                    let missing = a.arg("matchIfMissing").is_some_and(|v| v.text.trim() == "true");
                    if names.is_empty()
                        || !literal_strings(a, &["name", "value", "prefix", "havingValue"])
                        || a.arg("matchIfMissing").is_some_and(|v| !["true", "false"].contains(&v.text.trim()))
                        || a.args.iter().any(|v| {
                            matches!(v.name.as_str(), "prefix" | "havingValue" | "name" | "value")
                                && v.strings.is_empty()
                        })
                    {
                        Condition::Unknown
                    } else {
                        Condition::Property {
                            names: names
                                .into_iter()
                                .map(|n| {
                                    if prefix.is_empty() { n } else { format!("{}.{n}", prefix.trim_end_matches('.')) }
                                })
                                .collect(),
                            having,
                            missing,
                        }
                    }
                }
                "ConditionalOnMissingBean" => {
                    let types = classes(a, &["value"]);
                    let mut named_types = strings(a, &["type"]);
                    named_types.extend(types);
                    let names = strings(a, &["name"]);
                    if a.args.iter().any(|v| !["value", "type", "name"].contains(&v.name.as_str())) {
                        Condition::Unknown
                    } else {
                        Condition::Missing { types: named_types, names }
                    }
                }
                name if name.starts_with("Conditional") => Condition::Unknown,
                _ => return None,
            })
        })
        .collect()
}
impl Metadata {
    pub fn java(text: &str, file: &JavaFile, model: &FileModel) -> Self {
        let mut out = Self::default();
        for (i, ty) in file.types.iter().enumerate() {
            let name = file.qualified(i);
            let boot = ty.has(&["SpringBootApplication"]);
            let mut scans = Vec::new();
            let mut imports = Vec::new();
            let mut uncertain_scan = ty.supers().any(|s| {
                ["ImportSelector", "DeferredImportSelector", "ImportBeanDefinitionRegistrar"].contains(&s.name.as_str())
            });
            for annotation in &ty.annotations {
                if !file.from_spring(&annotation.name) {
                    continue;
                }
                if ["ComponentScan", "SpringBootApplication"].contains(&annotation.name.as_str()) {
                    let mut packages = strings(annotation, &["value", "basePackages", "scanBasePackages"]);
                    packages.extend(
                        classes(annotation, &["basePackageClasses", "scanBasePackageClasses"])
                            .iter()
                            .map(|n| qualify(file, n).rsplit_once('.').map_or(String::new(), |(p, _)| p.to_owned())),
                    );
                    uncertain_scan |= packages.iter().any(|p| p.contains('$'))
                        || annotation.args.iter().any(|a| {
                            ["excludeFilters", "includeFilters", "useDefaultFilters"].contains(&a.name.as_str())
                                || [
                                    "basePackages",
                                    "scanBasePackages",
                                    "basePackageClasses",
                                    "scanBasePackageClasses",
                                    "value",
                                ]
                                .contains(&a.name.as_str())
                                    && a.strings.is_empty()
                                    && !a.text.contains(".class")
                        });
                    if packages.is_empty() {
                        packages.push(file.package.clone());
                    }
                    scans.extend(packages);
                }
                if annotation.name == "Import" {
                    let imported = classes(annotation, &["value"]);
                    uncertain_scan |= imported.is_empty();
                    imports.extend(imported.iter().map(|n| qualify(file, n)));
                }
            }
            out.classes.push(Class {
                name: name.clone(),
                package: file.package.clone(),
                boot,
                auto: ty.has(&["AutoConfiguration"]),
                scans,
                imports,
                uncertain_scan,
                conditions: conditions(file, &ty.annotations),
            });
            let at = range(text, ty.at, ty.at + ty.name.len());
            if model.beans.iter().any(|b| b.range == at) {
                out.beans.push((at, name.clone(), vec![]));
            }
            for m in &ty.members {
                if m.kind == MemberKind::Method && m.has(&["Bean"]) {
                    out.beans.push((
                        range(text, m.at, m.at + m.name.len()),
                        name.clone(),
                        conditions(file, &m.annotations),
                    ));
                }
            }
        }
        out
    }
}

pub struct Environment {
    pub root: String,
    pub profiles: Option<BTreeSet<String>>,
    pub properties: BTreeMap<String, Option<String>>,
    pub complete: bool,
}
impl Environment {
    fn settings(classes: &[&Class]) -> Self {
        let root = host::setting_string("springBeans.applicationClass").unwrap_or_default();
        let roots: Vec<_> = classes.iter().filter(|c| c.boot).collect();
        let root = if root.is_empty() && roots.len() == 1 { roots[0].name.clone() } else { root };
        let profiles = host::setting_string("springBeans.activeProfiles").map(|p| {
            let set: BTreeSet<_> = p.split(',').map(str::trim).filter(|s| !s.is_empty()).map(str::to_owned).collect();
            if set.is_empty() { BTreeSet::from(["default".into()]) } else { set }
        });
        let raw = host::setting("springBeans.propertyOverrides").and_then(|s| serde_json::from_str::<Value>(&s).ok());
        let properties = match raw {
            Some(Value::Object(map)) => map.into_iter().map(|(k, v)| (k, v.as_str().map(str::to_owned))).collect(),
            Some(Value::String(text)) => {
                let mut map: BTreeMap<_, _> =
                    spring_text::config::properties(&text).into_iter().map(|k| (k.key, Some(k.value))).collect();
                for line in text.lines().filter_map(|s| s.trim().strip_prefix('!')) {
                    map.insert(line.trim().to_owned(), None);
                }
                map
            }
            _ => BTreeMap::new(),
        };
        Self {
            root,
            profiles,
            properties,
            complete: host::setting_bool("springBeans.environmentComplete").unwrap_or(false),
        }
    }
}

pub struct Index {
    states: BTreeMap<usize, State>,
    pub label: String,
}
impl Index {
    pub fn new(files: &BTreeMap<String, FileModel>) -> Self {
        let classes: Vec<_> = files.values().flat_map(|m| &m.context.classes).collect();
        let env = Environment::settings(&classes);
        let root = classes.iter().find(|c| c.name == env.root);
        let mut reachable = BTreeMap::new();
        let mut scans = BTreeMap::new();
        let mut uncertain = false;
        if let Some(root) = root {
            reachable.insert(root.name.clone(), State::Active);
        }
        loop {
            let mut changed = false;
            for class in &classes {
                let scanned = scans
                    .iter()
                    .filter(|(p, _)| class.package == **p || class.package.starts_with(&format!("{p}.")))
                    .fold(State::Inactive, |s, (_, state)| join(s, *state));
                if scanned != State::Inactive {
                    changed |= merge(&mut reachable, &class.name, scanned);
                }
                let state = reachable
                    .get(&class.name)
                    .copied()
                    .unwrap_or(State::Inactive)
                    .and(evaluate(&class.conditions, &env));
                if state != State::Inactive {
                    for package in &class.scans {
                        changed |= merge(
                            &mut scans,
                            package,
                            if class.uncertain_scan { state.and(State::Unknown) } else { state },
                        );
                    }
                    for name in &class.imports {
                        changed |= merge(&mut reachable, name, state);
                    }
                    uncertain |= class.uncertain_scan
                        || class.imports.iter().any(|name| !classes.iter().any(|c| c.name == *name));
                }
            }
            if !changed {
                break;
            }
        }
        let mut states = BTreeMap::new();
        let mut definitions = Vec::new();
        for model in files.values() {
            for bean in &model.beans {
                let meta = model.context.beans.iter().find(|(r, _, _)| *r == bean.range);
                let mut state = State::Active;
                let mut auto = false;
                let mut missing = vec![];
                if let Some((_, owner, conditions)) = meta {
                    if let Some(class) = classes.iter().find(|c| c.name == *owner) {
                        auto = class.auto;
                        if !env.root.is_empty() {
                            state = reachable.get(owner).copied().unwrap_or(if root.is_none() || uncertain || auto {
                                State::Unknown
                            } else {
                                State::Inactive
                            });
                        }
                        state = state.and(evaluate(&class.conditions, &env)).and(evaluate(conditions, &env));
                        missing.extend(
                            class
                                .conditions
                                .iter()
                                .chain(conditions)
                                .filter(|c| matches!(c, Condition::Missing { .. })),
                        );
                    }
                } else {
                    if bean.conditional {
                        state = State::Unknown;
                    }
                    if !bean.profiles.is_empty() {
                        state = state.and(evaluate(&[Condition::Profile(bean.profiles.clone())], &env));
                    }
                }
                let key = std::ptr::from_ref(bean) as usize;
                states.insert(key, state);
                definitions.push((key, bean, auto, missing));
            }
        }
        let unconditional = states.clone();
        for (key, bean, auto, missing) in &definitions {
            if !auto || missing.is_empty() || states[key] == State::Inactive {
                continue;
            }
            for condition in missing {
                let Condition::Missing { types, names } = condition else { continue };
                let types = if types.is_empty() && names.is_empty() { vec![bean.ty.clone()] } else { types.clone() };
                let present = definitions.iter().any(|(other, b, a, cs)| {
                    key != other
                        && !a
                        && cs.is_empty()
                        && unconditional[other] == State::Active
                        && (names.iter().any(|n| b.answers_to(n))
                            || types.iter().any(|t| is_a(files, &b.ty, t.rsplit('.').next().unwrap_or(t))))
                });
                if present {
                    states.insert(*key, State::Inactive);
                }
            }
        }
        Self {
            states,
            label: if env.root.is_empty() {
                "Source context: unselected".into()
            } else {
                format!(
                    "Source context: {}{}",
                    env.root,
                    env.profiles.map_or(" · profiles unknown".into(), |p| format!(
                        " · {}",
                        p.into_iter().collect::<Vec<_>>().join(",")
                    ))
                )
            },
        }
    }
    pub fn state(&self, bean: &Bean) -> State {
        self.states.get(&(std::ptr::from_ref(bean) as usize)).copied().unwrap_or(State::Unknown)
    }
}
fn join(a: State, b: State) -> State {
    if a == State::Active || b == State::Active {
        State::Active
    } else if a == State::Unknown || b == State::Unknown {
        State::Unknown
    } else {
        State::Inactive
    }
}
fn merge(map: &mut BTreeMap<String, State>, key: &str, state: State) -> bool {
    let old = map.get(key).copied().unwrap_or(State::Inactive);
    let value = join(old, state);
    if value == old {
        false
    } else {
        map.insert(key.to_owned(), value);
        true
    }
}

fn is_a(files: &BTreeMap<String, FileModel>, ty: &str, target: &str) -> bool {
    let mut todo = vec![ty];
    let mut seen = BTreeSet::new();
    while let Some(name) = todo.pop() {
        if name == target {
            return true;
        }
        if seen.insert(name) {
            todo.extend(
                files
                    .values()
                    .flat_map(|m| &m.types)
                    .filter(|t| t.name == name)
                    .flat_map(|t| &t.supers)
                    .map(String::as_str),
            );
        }
    }
    false
}

fn evaluate(conditions: &[Condition], env: &Environment) -> State {
    conditions.iter().fold(State::Active, |state, c| {
        state.and(match c {
            Condition::Unknown | Condition::Missing { .. } => State::Unknown,
            Condition::Profile(expressions) => env.profiles.as_ref().map_or(State::Unknown, |p| {
                let mut unknown = false;
                for expression in expressions {
                    match profile(expression, p) {
                        Some(true) => return State::Active,
                        None => unknown = true,
                        _ => {}
                    }
                }
                if unknown { State::Unknown } else { State::Inactive }
            }),
            Condition::Property { names, having, missing } => names.iter().fold(State::Active, |result, key| {
                result.and(match env.properties.get(key) {
                    Some(Some(value)) if value.contains("${") => State::Unknown,
                    Some(Some(value)) => {
                        if if having.is_empty() {
                            !value.eq_ignore_ascii_case("false")
                        } else {
                            value.eq_ignore_ascii_case(having)
                        } {
                            State::Active
                        } else {
                            State::Inactive
                        }
                    }
                    Some(None) => {
                        if *missing {
                            State::Active
                        } else {
                            State::Inactive
                        }
                    }
                    None if env.complete => {
                        if *missing {
                            State::Active
                        } else {
                            State::Inactive
                        }
                    }
                    None => State::Unknown,
                })
            }),
        })
    })
}

pub fn profile(text: &str, active: &BTreeSet<String>) -> Option<bool> {
    fn term(ts: &[String], i: &mut usize, active: &BTreeSet<String>) -> Option<bool> {
        let t = ts.get(*i)?;
        *i += 1;
        match t.as_str() {
            "!" => Some(!term(ts, i, active)?),
            "(" => {
                let v = expr(ts, i, active)?;
                if ts.get(*i)?.as_str() != ")" {
                    return None;
                }
                *i += 1;
                Some(v)
            }
            "&" | "|" | ")" => None,
            name => Some(active.contains(name)),
        }
    }
    fn expr(ts: &[String], i: &mut usize, active: &BTreeSet<String>) -> Option<bool> {
        let mut value = term(ts, i, active)?;
        let mut operator = None;
        while let Some(op) = ts.get(*i).filter(|s| s.as_str() == "&" || s.as_str() == "|") {
            if operator.is_some_and(|previous| previous != op) {
                return None;
            }
            operator = Some(op);
            *i += 1;
            let rhs = term(ts, i, active)?;
            value = if op == "&" { value && rhs } else { value || rhs };
        }
        Some(value)
    }
    let mut tokens = vec![];
    let mut name = String::new();
    for c in text.chars() {
        if "!&|()".contains(c) || c.is_whitespace() {
            if !name.is_empty() {
                tokens.push(std::mem::take(&mut name));
            }
            if !c.is_whitespace() {
                tokens.push(c.to_string());
            }
        } else {
            name.push(c);
        }
    }
    if !name.is_empty() {
        tokens.push(name);
    }
    let mut i = 0;
    let result = expr(&tokens, &mut i, active)?;
    (i == tokens.len()).then_some(result)
}
