//! Mutable JavaBean configuration properties only. Every Java symbol change is
//! a JDT request; resource edits retain the source's relaxed-binding spelling.
use super::{JavaRename, Kind, Plan, Site, Sources, annotation_is, exact_string, java_name};
use openidea_plugin::{Position, Range, TextEdit};
use spring_text::java::{self, Member, MemberKind};
use std::collections::{BTreeMap, BTreeSet};

const ANNOTATION: &str = "org.springframework.boot.context.properties.ConfigurationProperties";
pub(crate) const REFUSAL: &str = "Configuration property requires a unique mutable JavaBean field, getter and setter";

#[derive(Clone, Debug)]
struct Property {
    path: String,
    prefix: String,
    field: Member,
    getter: Member,
    setter: Member,
    members: Vec<String>,
}

#[derive(Clone, Debug)]
enum Origin {
    Key(String),
    Java(String),
}

#[derive(Clone, Debug)]
pub struct Selection {
    property: Property,
    origin: Origin,
    pub(crate) range: Range,
}

impl Selection {
    pub(crate) fn key(&self) -> String {
        format!("{}.{}", self.property.prefix, delimited(&self.property.field.name, '-'))
    }

    pub(crate) fn java_hint(&self, sources: &Sources) -> Option<JavaRename> {
        let Origin::Java(name) = &self.origin else { return None };
        let p = &self.property;
        let member = [&p.field, &p.getter, &p.setter].into_iter().find(|m| m.name == *name)?;
        Some(symbol(sources, p, member, None))
    }
}

fn normalized(name: &str) -> String {
    name.chars().filter(|c| !matches!(c, '-' | '_')).map(|c| c.to_ascii_lowercase()).collect()
}

fn normalized_path(name: &str) -> String {
    name.split('.').map(normalized).collect::<Vec<_>>().join(".")
}

pub(crate) fn under_prefix(name: &str, prefix: &str) -> bool {
    prefix.is_empty() || normalized_path(name).starts_with(&format!("{}.", normalized_path(prefix)))
}

fn capitalized(name: &str) -> String {
    let mut chars = name.chars();
    chars.next().map(|c| c.to_ascii_uppercase().to_string() + chars.as_str()).unwrap_or_default()
}

fn delimited(name: &str, delimiter: char) -> String {
    let chars: Vec<_> = name.chars().collect();
    let mut out = String::new();
    for (i, &c) in chars.iter().enumerate() {
        if c.is_ascii_uppercase()
            && i > 0
            && (chars[i - 1].is_ascii_lowercase()
                || chars[i - 1].is_ascii_digit()
                || chars.get(i + 1).is_some_and(char::is_ascii_lowercase))
        {
            out.push(delimiter);
        }
        out.push(c.to_ascii_lowercase());
    }
    out
}

fn java_property(name: &str) -> Option<String> {
    if !name.contains(['-', '_']) {
        return (java_name(name)
            && name.starts_with(|c: char| c.is_ascii_lowercase())
            && name.chars().all(|c| c.is_ascii_alphanumeric()))
        .then(|| name.to_owned());
    }
    let mut parts = name.split(['-', '_']);
    let mut out = parts.next()?.to_owned();
    for part in parts {
        if part.is_empty() {
            return None;
        }
        out.push_str(&capitalized(part));
    }
    java_property(&out)
}

fn spelling(old: &str, new_java: &str) -> String {
    if old.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_') {
        delimited(new_java, '_').to_ascii_uppercase()
    } else if old.contains('_') {
        delimited(new_java, '_')
    } else if old.contains('-') || !old.chars().any(|c| c.is_ascii_uppercase()) {
        delimited(new_java, '-')
    } else {
        new_java.to_owned()
    }
}

fn plain(member: &Member) -> bool {
    !member.modifiers.iter().any(|m| m == "static")
}

fn type_names(ty: &java::TypeRef, names: &mut BTreeSet<String>) {
    names.insert(ty.name.clone());
    for arg in &ty.args {
        type_names(arg, names);
    }
}

fn scalar(file: &java::JavaFile, member: &Member, declared: &BTreeSet<String>) -> bool {
    member.ty.as_ref().is_some_and(|t| {
        !t.array
            && t.args.is_empty()
            && [
                "String",
                "boolean",
                "Boolean",
                "byte",
                "Byte",
                "short",
                "Short",
                "int",
                "Integer",
                "long",
                "Long",
                "float",
                "Float",
                "double",
                "Double",
                "char",
                "Character",
                "Duration",
                "DataSize",
            ]
            .contains(&t.name.as_str())
            && {
                let qualified = match t.name.as_str() {
                    "Duration" => format!("java.time.{}", t.name),
                    "DataSize" => format!("org.springframework.util.unit.{}", t.name),
                    _ if t.name.starts_with(char::is_uppercase) => format!("java.lang.{}", t.name),
                    _ => t.name.clone(),
                };
                let local =
                    if file.package.is_empty() { t.name.clone() } else { format!("{}.{}", file.package, t.name) };
                (t.text == t.name || t.text == qualified)
                    && (t.text == qualified || !declared.contains(&local))
                    && (!["Duration", "DataSize"].contains(&t.name.as_str())
                        || t.text == qualified
                        || file.imports.iter().any(|i| {
                            i.path == qualified || i.path == format!("{}.*", qualified.rsplit_once('.').unwrap().0)
                        }))
                    && !file
                        .imports
                        .iter()
                        .any(|i| !i.is_static && i.path.ends_with(&format!(".{}", t.name)) && i.path != qualified)
            }
    })
}

fn canonical_prefix(prefix: &str) -> bool {
    !prefix.is_empty()
        && prefix.split('.').all(|part| {
            part.starts_with(|c: char| c.is_ascii_lowercase())
                && !part.ends_with('-')
                && part.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        })
}

struct Index {
    properties: Vec<Property>,
    keys: BTreeMap<String, usize>,
    unknown: Vec<String>,
    inherited: BTreeSet<String>,
    unresolved_prefix: bool,
}

impl Index {
    fn new(sources: &Sources) -> Self {
        let mut out = Self {
            properties: Vec::new(),
            keys: BTreeMap::new(),
            unknown: Vec::new(),
            inherited: BTreeSet::new(),
            unresolved_prefix: false,
        };
        let files: Vec<_> = sources
            .iter()
            .filter(|(p, _)| p.ends_with(".java"))
            .map(|(path, text)| (path, text, java::parse(text)))
            .collect();
        let declared: BTreeSet<_> =
            files.iter().flat_map(|(_, _, file)| (0..file.types.len()).map(|i| file.qualified(i))).collect();
        for (path, text, file) in files.iter().map(|(p, t, file)| (*p, *t, file)) {
            for (ti, t) in file.types.iter().enumerate() {
                for method in t.methods() {
                    if let Some(annotation) =
                        method.annotations.iter().find(|a| annotation_is(file, text, a, ANNOTATION))
                    {
                        if let Some(ty) = &method.ty {
                            type_names(ty, &mut out.inherited);
                        }
                        let names = annotation.strings(&["prefix", "value"]);
                        let literal = annotation.arg("prefix").or_else(|| annotation.arg("value"));
                        if names.len() == 1
                            && exact_string(text, names[0])
                            && canonical_prefix(&names[0].value)
                            && literal.is_some_and(|a| java::tokens(&a.text).len() == 1)
                        {
                            out.unknown.push(normalized_path(&names[0].value));
                            if let Some(ty) = &method.ty {
                                out.inherited.insert(ty.name.clone());
                            }
                        } else {
                            out.unresolved_prefix = true;
                        }
                    }
                }
                let Some(annotation) = t.annotations.iter().find(|a| annotation_is(file, text, a, ANNOTATION)) else {
                    continue;
                };
                let strings = annotation.strings(&["prefix", "value"]);
                if strings.len() != 1 {
                    out.unresolved_prefix = true;
                    continue;
                }
                let prefix = &strings[0].value;
                if !exact_string(text, strings[0]) || !canonical_prefix(prefix) {
                    out.unresolved_prefix = true;
                    continue;
                }
                let literal = annotation.arg("prefix").or_else(|| annotation.arg("value"));
                if literal.is_none_or(|a| java::tokens(&a.text).len() != 1) {
                    out.unresolved_prefix = true;
                    continue;
                }
                let mutable = t.kind == "class"
                    && t.outer.is_none()
                    && t.extends.is_empty()
                    && t.implements.is_empty()
                    && !t.modifiers.iter().any(|m| m == "abstract")
                    && !t.members.iter().any(|m| m.kind == MemberKind::Constructor && !m.params.is_empty())
                    && !t
                        .annotations
                        .iter()
                        .chain(t.members.iter().flat_map(|m| &m.annotations))
                        .any(|a| a.name == "ConstructorBinding")
                    && !declared.contains(&format!("{}BeanInfo", file.qualified(ti)));
                if !mutable {
                    out.unknown.push(normalized_path(prefix));
                    out.inherited.extend(t.supers().map(|t| t.name.clone()));
                }
                for field in t.fields().filter(|f| plain(f)) {
                    if !scalar(file, field, &declared)
                        && let Some(ty) = &field.ty
                    {
                        type_names(ty, &mut out.inherited);
                    }
                    let key = normalized_path(&format!("{prefix}.{}", field.name));
                    *out.keys.entry(key).or_default() += 1;
                    if !mutable
                        || !scalar(file, field, &declared)
                        || field.modifiers.iter().any(|m| m == "final")
                        || java_property(&field.name).as_deref() != Some(&field.name)
                        || field
                            .annotations
                            .iter()
                            .any(|a| ["Value", "NestedConfigurationProperty"].contains(&a.name.as_str()))
                    {
                        continue;
                    }
                    let cap = capitalized(&field.name);
                    let getters: Vec<_> = t
                        .methods()
                        .filter(|m| {
                            (m.name == format!("get{cap}")
                                || field.ty.as_ref().is_some_and(|t| t.name == "boolean")
                                    && m.name == format!("is{cap}"))
                                && plain(m)
                                && m.modifiers.iter().any(|m| m == "public")
                                && m.params.is_empty()
                                && scalar(file, m, &declared)
                                && m.ty.as_ref().map(|t| &t.name) == field.ty.as_ref().map(|t| &t.name)
                        })
                        .collect();
                    let setters: Vec<_> = t.methods().filter(|m| m.name == format!("set{cap}")).collect();
                    if getters.len() != 1 || setters.len() != 1 {
                        continue;
                    }
                    let setter = setters[0];
                    if !plain(setter)
                        || !setter.modifiers.iter().any(|m| m == "public")
                        || setter.params.len() != 1
                        || setter.params[0].ty.name != field.ty.as_ref().unwrap().name
                        || setter.params[0].ty.array
                        || !setter.params[0].ty.args.is_empty()
                        || setter.params[0].ty.text != field.ty.as_ref().unwrap().text
                        || setter.ty.as_ref().is_none_or(|t| t.name != "void")
                    {
                        continue;
                    }
                    out.properties.push(Property {
                        path: path.clone(),
                        prefix: prefix.clone(),
                        field: field.clone(),
                        getter: getters[0].clone(),
                        setter: setter.clone(),
                        members: t.members.iter().map(|m| m.name.clone()).collect(),
                    });
                }
            }
        }
        loop {
            let before = out.inherited.len();
            for (_, _, file) in &files {
                for t in &file.types {
                    if out.inherited.contains(&t.name) {
                        out.inherited.extend(t.supers().map(|ty| ty.name.clone()));
                        for field in t.fields() {
                            if let Some(ty) = &field.ty {
                                type_names(ty, &mut out.inherited);
                            }
                        }
                    }
                }
            }
            if before == out.inherited.len() {
                break;
            }
        }
        out
    }

    fn unique(&self, p: &Property) -> bool {
        let key = normalized_path(&format!("{}.{}", p.prefix, p.field.name));
        !self.unresolved_prefix
            && self.keys.get(&key) == Some(&1)
            && !self.unknown.iter().any(|prefix| key.starts_with(&format!("{prefix}.")))
    }
}

pub(crate) fn unresolved_prefix(sources: &Sources) -> bool {
    Index::new(sources).unresolved_prefix
}

pub(crate) fn select(sources: &Sources, sites: &[Site], path: &str, pos: Position) -> Option<Selection> {
    let text = sources.get(path)?;
    let at = openidea_plugin::text::offset(text, pos)?;
    let index = Index::new(sources);
    for p in index.properties.iter().filter(|p| index.unique(p)) {
        if p.path == path {
            for m in [&p.field, &p.getter, &p.setter] {
                if m.at <= at && at <= m.at + m.name.len() {
                    return Some(Selection {
                        property: p.clone(),
                        origin: Origin::Java(m.name.clone()),
                        range: spring_text::range(text, m.at, m.at + m.name.len()),
                    });
                }
            }
        }
        let key = normalized_path(&format!("{}.{}", p.prefix, p.field.name));
        if let Some(site) = sites.iter().find(|s| {
            s.path == path
                && s.from <= at
                && at <= s.to
                && matches!(s.kind, Kind::ConfigDef | Kind::ConfigRef)
                && normalized_path(&s.name) == key
        }) {
            return Some(Selection {
                property: p.clone(),
                origin: Origin::Key(site.name.clone()),
                range: spring_text::range(text, site.from, site.to),
            });
        }
    }
    None
}

pub(crate) fn inherited_declaration(sources: &Sources, path: &str, pos: Position) -> bool {
    let Some(text) = sources.get(path).filter(|_| path.ends_with(".java")) else { return false };
    let Some(at) = openidea_plugin::text::offset(text, pos) else { return false };
    let index = Index::new(sources);
    java::parse(text).types.iter().any(|t| {
        index.inherited.contains(&t.name)
            && t.members.iter().any(|m| {
                (m.kind == MemberKind::Field
                    || m.name.starts_with("get")
                    || m.name.starts_with("set")
                    || m.name.starts_with("is"))
                    && m.at <= at
                    && at <= m.at + m.name.len()
            })
    })
}

fn symbol(sources: &Sources, p: &Property, member: &Member, new_name: Option<String>) -> JavaRename {
    JavaRename {
        path: p.path.clone(),
        pos: openidea_plugin::text::position(&sources[&p.path], member.at),
        old_name: member.name.clone(),
        new_name,
    }
}

pub(crate) fn plan(
    sources: &Sources,
    sites: &[Site],
    selection: &Selection,
    replacement: &str,
) -> Result<Plan, &'static str> {
    let p = &selection.property;
    let index = Index::new(sources);
    if !index.unique(p) {
        return Err(REFUSAL);
    }
    let new_java = match &selection.origin {
        Origin::Key(old) => {
            let leaf = if let Some((parent, leaf)) = replacement.rsplit_once('.') {
                if Some(parent) != old.rsplit_once('.').map(|(parent, _)| parent) {
                    return Err("Configuration key must stay in the same parent mapping");
                }
                leaf
            } else {
                replacement
            };
            java_property(leaf).ok_or("Invalid configuration key")?
        }
        Origin::Java(name) if name == &p.field.name => {
            java_property(replacement).filter(|name| name == replacement).ok_or("Invalid Java identifier")?
        }
        Origin::Java(name) => {
            let prefix = if name == &p.getter.name { if name.starts_with("is") { "is" } else { "get" } } else { "set" };
            let tail = replacement
                .strip_prefix(prefix)
                .filter(|s| s.starts_with(|c: char| c.is_ascii_uppercase()))
                .ok_or("Invalid JavaBean accessor name")?;
            let property = spring_text::decapitalize(tail);
            java_property(&property).ok_or("Invalid JavaBean accessor name")?
        }
    };
    if new_java == p.field.name {
        return Ok(Plan::default());
    }
    let cap = capitalized(&new_java);
    let new_getter = format!("{}{cap}", if p.getter.name.starts_with("is") { "is" } else { "get" });
    let new_setter = format!("set{cap}");
    if [&new_java, &new_getter, &new_setter].iter().any(|name| p.members.contains(name)) {
        return Err("The target identity already exists in this source scope");
    }
    let old_key = normalized_path(&format!("{}.{}", p.prefix, p.field.name));
    let new_key = normalized_path(&format!("{}.{}", p.prefix, new_java));
    if index.keys.contains_key(&new_key)
        || sites.iter().any(|s| {
            matches!(s.kind, Kind::ConfigDef | Kind::ConfigRef)
                && (normalized_path(&s.name) == new_key
                    || normalized_path(&s.name).starts_with(&format!("{new_key}."))
                    || normalized_path(&s.name).starts_with(&format!("{old_key}.")))
        })
    {
        return Err("The target identity already exists in this source scope");
    }
    let mut out = Plan {
        java: Some(symbol(sources, p, &p.field, Some(new_java.clone()))),
        related_java: vec![
            symbol(sources, p, &p.getter, Some(new_getter)),
            symbol(sources, p, &p.setter, Some(new_setter)),
        ],
        ..Default::default()
    };
    for s in sites
        .iter()
        .filter(|s| matches!(s.kind, Kind::ConfigDef | Kind::ConfigRef) && normalized_path(&s.name) == old_key)
    {
        let text = &sources[&s.path];
        let (parent, leaf) = s.name.rsplit_once('.').ok_or(REFUSAL)?;
        let new_leaf = spelling(leaf, &new_java);
        let raw = &text[s.from..s.to];
        let value = if s.kind == Kind::ConfigDef && raw == leaf { new_leaf } else { format!("{parent}.{new_leaf}") };
        out.files
            .entry(s.path.clone())
            .or_default()
            .push(TextEdit { range: spring_text::range(text, s.from, s.to), text: value });
    }
    Ok(out)
}
