//! The only bridge to dependency binaries: cached JDT symbols from an authorized build path.
use crate::{Bean, Injection, context};
use openidea_plugin::{Position, Range};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

pub struct Entry {
    pub path: String,
    pub bean: Bean,
    state: context::State,
    supers: BTreeSet<String>,
    generic: bool,
    generic_types: BTreeSet<String>,
    pub parameters: Vec<String>,
}
#[derive(Default)]
pub struct Snapshot {
    pub entries: Vec<Entry>,
    by_pointer: BTreeMap<usize, usize>,
    injections: BTreeMap<(String, Position), String>,
    source_types: BTreeMap<(String, Range), BTreeSet<String>>,
    pub incomplete: bool,
    pub available: bool,
}
fn range(value: &Value) -> Option<Range> {
    let pos = |v: &Value| {
        Some(Position::new(v["line"].as_u64()?.try_into().ok()?, v["character"].as_u64()?.try_into().ok()?))
    };
    Some(Range::new(pos(&value["start"])?, pos(&value["end"])?))
}
fn strings(v: &Value) -> Vec<String> {
    v.as_array().into_iter().flatten().filter_map(Value::as_str).map(str::to_owned).collect()
}
impl Snapshot {
    pub fn load(path: Option<&str>, selected: &str, files: &BTreeMap<String, crate::FileModel>) -> Self {
        let Some(path) = path else { return Self::default() };
        let Some(project) = spring_text::index::project_of(path) else { return Self::default() };
        let Some(json) = project.spring_beans(path, selected).ok().flatten() else { return Self::default() };
        Self::parse(&json, files)
    }
    pub fn parse(json: &str, files: &BTreeMap<String, crate::FileModel>) -> Self {
        let Ok(value) = serde_json::from_str::<Value>(json) else { return Self::default() };
        let mut out =
            Self { available: true, incomplete: value["incomplete"].as_bool().unwrap_or(true), ..Self::default() };
        for i in value["injections"].as_array().into_iter().flatten().take(12000) {
            if let (Some(path), Some(r), Some(ty)) =
                (i["location"]["path"].as_str(), range(&i["location"]["range"]), i["type"].as_str())
            {
                out.injections.insert((path.into(), r.start), ty.into());
            }
        }
        for b in value["beans"].as_array().into_iter().flatten().take(12000) {
            let (Some(path), Some(r), Some(name), Some(ty)) = (
                b["location"]["path"].as_str(),
                range(&b["location"]["range"]),
                b["name"].as_str(),
                b["type"].as_str(),
            ) else {
                out.incomplete = true;
                continue;
            };
            // Local source remains owned by the context/configuration provider.
            if files.contains_key(path) {
                let mut supers: BTreeSet<_> = strings(&b["supers"]).into_iter().collect();
                supers.insert(ty.into());
                out.source_types.insert((path.into(), r), supers);
                continue;
            }
            let state = match b["state"].as_str() {
                Some("active") => context::State::Active,
                Some("unknown") => context::State::Unknown,
                _ => {
                    out.incomplete = true;
                    continue;
                }
            };
            let origin = if b["origin"] == "source" { "module" } else { "dependency" };
            out.entries.push(Entry {
                path: path.into(),
                state,
                supers: strings(&b["supers"]).into_iter().collect(),
                generic: b["generic"].as_bool().unwrap_or(true),
                generic_types: strings(&b["genericTypes"]).into_iter().collect(),
                parameters: strings(&b["dependencies"]),
                bean: Bean {
                    name: name.into(),
                    ty: ty.into(),
                    aliases: strings(&b["aliases"]),
                    qualifiers: strings(&b["qualifiers"]),
                    kind: if b["kind"] == "factory" {
                        format!("@Bean ({origin})")
                    } else {
                        format!("@Component ({origin})")
                    },
                    primary: b["primary"].as_bool().unwrap_or(false),
                    conditional: state == context::State::Unknown,
                    profiles: vec![],
                    is_abstract: false,
                    range: r,
                },
            });
        }
        out.by_pointer =
            out.entries.iter().enumerate().map(|(i, e)| (std::ptr::from_ref(&e.bean) as usize, i)).collect();
        out
    }
    pub fn entry(&self, bean: &Bean) -> Option<&Entry> {
        self.by_pointer.get(&(std::ptr::from_ref(bean) as usize)).map(|i| &self.entries[*i])
    }
    pub fn state(&self, bean: &Bean) -> Option<context::State> {
        self.entry(bean).map(|e| e.state)
    }
    pub fn matches(&self, path: &str, inj: &Injection, bean: &Bean) -> Option<bool> {
        let entry = self.entry(bean)?;
        // Missing/recovered injection bindings never fall back to an ambiguous simple name.
        let ty = self.injections.get(&(path.into(), inj.range.start))?;
        Some(entry.supers.contains(ty) || bean.ty == *ty)
    }
    pub fn has_type(&self, path: &str, bean: &Bean, ty: &str) -> bool {
        if let Some(entry) = self.entry(bean) {
            return entry.supers.contains(ty) || bean.ty == ty;
        }
        self.source_types.get(&(path.into(), bean.range)).is_some_and(|supers| supers.contains(ty))
    }
    pub fn source_matches(&self, path: &str, bean_path: &str, inj: &Injection, bean: &Bean) -> Option<bool> {
        let ty = self.injections.get(&(path.into(), inj.range.start))?;
        self.source_types.get(&(bean_path.into(), bean.range)).map(|supers| supers.contains(ty))
    }
    pub fn uncertain_for(&self, path: Option<&str>, inj: &Injection, bean: &Bean) -> bool {
        let Some(entry) = self.entry(bean) else { return false };
        let ty = path.and_then(|p| self.injections.get(&(p.into(), inj.range.start)));
        entry.generic
            && (entry.generic_types.is_empty()
                || entry.generic_types.contains("*")
                || ty.is_none_or(|ty| entry.generic_types.contains(ty)))
    }
}
