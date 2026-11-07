//! Source-only Spring identities. No file writes: callers merge Java LSP edits,
//! capture snapshot preconditions, preview, then use the workspace transaction.
use openidea_plugin::{Position, Range, TextEdit};
use spring_text::{config, java, xml};
use std::collections::BTreeMap;
mod properties;
pub use properties::Selection as PropertySelection;
mod java_edits;
pub use java_edits::refine_java_edit;

pub type Sources = BTreeMap<String, String>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Identity {
    Bean(String),
    Config(String),
    Statement { namespace: String, name: String },
}

#[derive(Clone, Debug)]
pub struct Target {
    pub identity: Identity,
    pub range: Range,
    /// Java declarations must be renamed by JDT, including their Java usages.
    pub java: Option<JavaRename>,
    pub property: Option<PropertySelection>,
}

#[derive(Clone, Debug)]
pub struct JavaRename {
    pub path: String,
    pub pos: Position,
    pub old_name: String,
    pub new_name: Option<String>,
}

#[derive(Default, Debug)]
pub struct Plan {
    pub files: BTreeMap<String, Vec<TextEdit>>,
    pub java: Option<JavaRename>,
    pub related_java: Vec<JavaRename>,
}

impl Plan {
    pub fn java_requests(&self) -> impl Iterator<Item = &JavaRename> {
        self.java.iter().chain(&self.related_java)
    }
}

#[derive(Clone)]
struct Site {
    path: String,
    name: String,
    from: usize,
    to: usize,
    kind: Kind,
}

#[derive(Clone, PartialEq, Eq)]
enum Kind {
    BeanDef,
    BeanRef,
    QualifierTag,
    ConfigDef,
    ConfigRef,
    Prefix,
    StatementDef(String),
    StatementRef(String),
    JavaStatement(String),
    ImplicitBean { bean: String, class: bool },
    UnsupportedBinding,
}

fn exact_string(text: &str, s: &java::Str) -> bool {
    text.get(s.at..s.end) == Some(s.value.as_str()) && !s.value.contains(['\\', '"', '\n', '\r'])
}

// Requiring import/FQN evidence avoids treating an unrelated @Value/@Bean as Spring.
fn annotation_is(file: &java::JavaFile, text: &str, a: &java::Annotation, qualified: &str) -> bool {
    if qualified.rsplit('.').next() != Some(a.name.as_str()) {
        return false;
    }
    let written = text[a.at + 1..a.name_at + a.name.len()].trim();
    if written.contains('.') {
        return written == qualified;
    }
    if file.types.iter().any(|t| t.name == a.name) {
        return false;
    }
    let package = qualified.rsplit_once('.').unwrap().0;
    file.imports.iter().any(|i| !i.is_static && (i.path == qualified || i.path == format!("{package}.*")))
        && !file
            .imports
            .iter()
            .any(|i| !i.is_static && i.path.ends_with(&format!(".{}", a.name)) && i.path != qualified)
}

fn placeholders(value: &str, base: usize) -> Vec<(String, usize, usize)> {
    let mut out = Vec::new();
    for (i, _) in value.match_indices("${") {
        // Escaped placeholders and expression/dynamic keys are outside the supported scope.
        if i > 0 && value.as_bytes()[i - 1] == b'\\' {
            continue;
        }
        let from = i + 2;
        let end = value[from..].find([':', '}']).map(|j| from + j);
        if let Some(end) = end {
            let key = &value[from..end];
            if config_name(key) {
                out.push((key.to_owned(), base + from, base + end));
            }
        }
    }
    out
}

fn config_name(s: &str) -> bool {
    !s.is_empty()
        && s.split('.').all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_')))
}

fn simple_name(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '$'))
}

fn java_name(s: &str) -> bool {
    simple_name(s)
        && !s.contains('-')
        && !s.starts_with(|c: char| c.is_ascii_digit())
        && ![
            "abstract",
            "assert",
            "boolean",
            "break",
            "byte",
            "case",
            "catch",
            "char",
            "class",
            "const",
            "continue",
            "default",
            "do",
            "double",
            "else",
            "enum",
            "extends",
            "final",
            "finally",
            "float",
            "for",
            "goto",
            "if",
            "implements",
            "import",
            "instanceof",
            "int",
            "interface",
            "long",
            "native",
            "new",
            "package",
            "private",
            "protected",
            "public",
            "return",
            "short",
            "static",
            "strictfp",
            "super",
            "switch",
            "synchronized",
            "this",
            "throw",
            "throws",
            "transient",
            "try",
            "void",
            "volatile",
            "while",
            "null",
            "true",
            "false",
        ]
        .contains(&s)
}

fn sites(sources: &Sources) -> Vec<Site> {
    let mut out = Vec::new();
    for (path, text) in sources {
        let mut push = |name: String, from, to, kind| out.push(Site { path: path.clone(), name, from, to, kind });
        if path.ends_with(".java") {
            let file = java::parse(text);
            let toks = java::tokens(text);
            let mut annotations = Vec::new();
            let mut i = 0;
            while i < toks.len() {
                if toks[i].s == "@" {
                    i = java::annotation(text, &toks, i, &mut annotations).max(i + 1);
                } else {
                    i += 1;
                }
            }
            for a in &annotations {
                let kind = match a.name.as_str() {
                    "Bean" if annotation_is(&file, text, a, "org.springframework.context.annotation.Bean") => {
                        Some(Kind::BeanDef)
                    }
                    "Component" | "Service" | "Repository" | "Controller"
                        if annotation_is(&file, text, a, &format!("org.springframework.stereotype.{}", a.name)) =>
                    {
                        Some(Kind::BeanDef)
                    }
                    "Qualifier"
                        if annotation_is(&file, text, a, "org.springframework.beans.factory.annotation.Qualifier") =>
                    {
                        // A qualifier on a bean declaration is a tag, independent of its name.
                        let tag = file.types.iter().any(|t| {
                            t.annotations.iter().any(|x| x.at == a.at)
                                || t.methods().any(|m| m.has(&["Bean"]) && m.annotations.iter().any(|x| x.at == a.at))
                        });
                        Some(if tag { Kind::QualifierTag } else { Kind::BeanRef })
                    }
                    "Resource" if annotation_is(&file, text, a, "javax.annotation.Resource") => Some(Kind::BeanRef),
                    "ConfigurationProperties"
                        if annotation_is(
                            &file,
                            text,
                            a,
                            "org.springframework.boot.context.properties.ConfigurationProperties",
                        ) =>
                    {
                        Some(Kind::Prefix)
                    }
                    _ => None,
                };
                if let Some(kind) = kind {
                    let names = match a.name.as_str() {
                        "Bean" => &["name", "value"][..],
                        "Resource" => &["name"][..],
                        "ConfigurationProperties" => &["prefix", "value"][..],
                        _ => &["value"][..],
                    };
                    for s in a.strings(names) {
                        // Constants/concatenation are never edited as if they were literal names.
                        let literal_arg = names.iter().find_map(|n| a.arg(n));
                        let valid_arg = literal_arg.is_some_and(|v| {
                            let raw = v.text.trim();
                            (v.strings.len() == 1
                                && raw.starts_with('"')
                                && raw.ends_with('"')
                                && java::tokens(raw).len() == 1)
                                || (a.name == "Bean"
                                    && raw.starts_with('{')
                                    && raw.ends_with('}')
                                    && !raw.contains('+'))
                        });
                        if exact_string(text, s) && valid_arg {
                            push(s.value.clone(), s.at, s.end, kind.clone());
                        }
                    }
                }
                if a.name == "Value"
                    && annotation_is(&file, text, a, "org.springframework.beans.factory.annotation.Value")
                {
                    for s in a.strings(&["value"]) {
                        if exact_string(text, s) {
                            for (key, from, to) in placeholders(&s.value, s.at) {
                                push(key, from, to, Kind::ConfigRef);
                            }
                        }
                    }
                }
            }
            for (ti, t) in file.types.iter().enumerate() {
                if t.annotations.iter().any(|a| {
                    annotation_is(&file, text, a, "org.springframework.boot.context.properties.ConfigurationProperties")
                }) {
                    for component in &t.components {
                        push(
                            component.name.clone(),
                            component.at,
                            component.at + component.name.len(),
                            Kind::UnsupportedBinding,
                        );
                    }
                    for member in &t.members {
                        if member.kind == java::MemberKind::Field
                            || member.name.starts_with("get")
                            || member.name.starts_with("set")
                            || member.name.starts_with("is")
                        {
                            push(
                                member.name.clone(),
                                member.at,
                                member.at + member.name.len(),
                                Kind::UnsupportedBinding,
                            );
                        }
                    }
                }
                let stereotype = t.annotations.iter().find(|a| {
                    ["Component", "Service", "Repository", "Controller"].contains(&a.name.as_str())
                        && annotation_is(&file, text, a, &format!("org.springframework.stereotype.{}", a.name))
                });
                if stereotype.is_some_and(|a| a.args.is_empty()) {
                    push(
                        t.name.clone(),
                        t.at,
                        t.at + t.name.len(),
                        Kind::ImplicitBean { bean: spring_text::decapitalize(&t.name), class: true },
                    );
                }
                for m in t.methods() {
                    if m.annotations.iter().any(|a| {
                        a.args.is_empty()
                            && annotation_is(&file, text, a, "org.springframework.context.annotation.Bean")
                    }) {
                        push(
                            m.name.clone(),
                            m.at,
                            m.at + m.name.len(),
                            Kind::ImplicitBean { bean: m.name.clone(), class: false },
                        );
                    }
                    if t.kind == "interface" && m.body.is_none() && !m.modifiers.iter().any(|m| m == "static") {
                        push(m.name.clone(), m.at, m.at + m.name.len(), Kind::JavaStatement(file.qualified(ti)));
                    }
                }
            }
        } else if config::is_config_file(path) {
            let keys = if path.ends_with(".properties") { config::properties(text) } else { config::yaml(text) };
            for key in keys {
                if !config_name(&key.key) || text[key.at..key.end].contains('\\') {
                    continue;
                }
                push(key.key, key.at, key.end, Kind::ConfigDef);
                for (name, from, to) in placeholders(&key.value, key.value_at) {
                    push(name, from, to, Kind::ConfigRef);
                }
            }
        } else if path.ends_with(".xml") {
            let elements = xml::elements(text);
            if xml::root(&elements) == Some("mapper") {
                let Some(ns) = elements[0].value("namespace") else { continue };
                for e in &elements {
                    if ["select", "insert", "update", "delete"].contains(&e.local())
                        && e.parent == Some(0)
                        && let Some(id) = e.attr("id")
                    {
                        push(id.value.clone(), id.value_at, id.value_end, Kind::StatementDef(ns.to_owned()));
                    }
                    // Nested query references are statement identities. resultMap/refid/SQL text are not.
                    if ["association", "collection", "id", "result", "arg", "idArg"].contains(&e.local())
                        && let Some(a) = e.attr("select")
                    {
                        let (namespace, name) = a.value.rsplit_once('.').unwrap_or((ns, &a.value));
                        let from = a.value_end - name.len();
                        push(name.to_owned(), from, a.value_end, Kind::StatementRef(namespace.to_owned()));
                    }
                }
            } else if xml::root(&elements) == Some("beans")
                && xml::namespace(&elements, 0) == Some("http://www.springframework.org/schema/beans")
            {
                for (i, e) in elements.iter().enumerate() {
                    if xml::namespace(&elements, i) != Some("http://www.springframework.org/schema/beans") {
                        continue;
                    }
                    for a in &e.attrs {
                        let kind = match (e.local(), a.name.as_str()) {
                            ("bean", "id" | "name") | ("alias", "alias") => Some(Kind::BeanDef),
                            ("ref", "bean" | "local")
                            | ("idref", "bean" | "local")
                            | ("alias", "name")
                            | ("property" | "constructor-arg", "ref")
                            | ("bean", "depends-on" | "factory-bean" | "parent") => Some(Kind::BeanRef),
                            _ => None,
                        };
                        if let Some(kind) = kind {
                            let mut base = a.value_at;
                            for part in a.value.split_inclusive(|c: char| c.is_whitespace() || c == ',' || c == ';') {
                                let name = part.trim_end_matches(|c: char| c.is_whitespace() || c == ',' || c == ';');
                                if !name.is_empty() {
                                    push(name.to_owned(), base, base + name.len(), kind.clone());
                                }
                                base += part.len();
                            }
                        }
                    }
                }
            }
        }
    }
    out
}

fn same_family(kind: &Kind, identity: &Identity) -> bool {
    match identity {
        Identity::Bean(_) => matches!(kind, Kind::BeanDef | Kind::BeanRef),
        Identity::Config(_) => matches!(kind, Kind::ConfigDef | Kind::ConfigRef | Kind::Prefix),
        Identity::Statement { namespace, .. } => {
            matches!(kind, Kind::StatementDef(ns) | Kind::StatementRef(ns) | Kind::JavaStatement(ns) if ns == namespace)
        }
    }
}

/// Only identities proved by declarations in this source set are offered.
pub fn target(sources: &Sources, path: &str, pos: Position) -> Option<Target> {
    let all = sites(sources);
    if let Some(property) = properties::select(sources, &all, path, pos) {
        return Some(Target {
            identity: Identity::Config(property.key()),
            range: property.range,
            java: property.java_hint(sources),
            property: Some(property),
        });
    }
    let text = sources.get(path)?;
    let at = openidea_plugin::text::offset(text, pos)?;
    let site = all.iter().find(|s| s.path == path && s.from <= at && at <= s.to)?;
    let java = matches!(site.kind, Kind::JavaStatement(_) | Kind::ImplicitBean { .. }).then(|| JavaRename {
        path: path.to_owned(),
        pos: openidea_plugin::text::position(text, site.from),
        old_name: site.name.clone(),
        new_name: None,
    });
    let identity = match &site.kind {
        Kind::BeanDef | Kind::BeanRef => Identity::Bean(site.name.clone()),
        Kind::ImplicitBean { bean, .. } => Identity::Bean(bean.clone()),
        Kind::ConfigDef | Kind::ConfigRef | Kind::Prefix => Identity::Config(site.name.clone()),
        Kind::StatementDef(ns) | Kind::StatementRef(ns) | Kind::JavaStatement(ns) => {
            if !all.iter().any(|s| s.name == site.name && s.kind == Kind::StatementDef(ns.clone())) {
                return None;
            }
            Identity::Statement { namespace: ns.clone(), name: site.name.clone() }
        }
        Kind::QualifierTag | Kind::UnsupportedBinding => return None,
    };
    if let Identity::Bean(name) = &identity {
        let defs = all
            .iter()
            .filter(|s| {
                s.name == *name && s.kind == Kind::BeanDef
                    || matches!(&s.kind, Kind::ImplicitBean { bean, .. } if bean == name)
            })
            .count();
        if defs != 1 || all.iter().any(|s| s.name == *name && s.kind == Kind::QualifierTag) {
            return None;
        }
    }
    Some(Target { identity, range: spring_text::range(text, site.from, site.to), java, property: None })
}

/// A recognized but unresolved source identity must not fall back to a textual
/// XML/YAML rename, which could silently omit the Java side.
pub fn recognized(sources: &Sources, path: &str, pos: Position) -> bool {
    let Some(text) = sources.get(path) else { return false };
    let Some(at) = openidea_plugin::text::offset(text, pos) else { return false };
    let all = sites(sources);
    properties::inherited_declaration(sources, path, pos)
        || all.iter().any(|s| {
            s.path == path
                && s.from <= at
                && at <= s.to
                && match &s.kind {
                    Kind::JavaStatement(ns) => all.iter().any(|x| matches!(&x.kind, Kind::StatementDef(n) if n == ns)),
                    _ => true,
                }
        })
}

pub fn refusal(sources: &Sources, path: &str, pos: Position) -> &'static str {
    let at = sources.get(path).and_then(|text| openidea_plugin::text::offset(text, pos));
    if properties::inherited_declaration(sources, path, pos)
        || at.is_some_and(|at| {
            sites(sources)
                .iter()
                .any(|s| s.path == path && s.from <= at && at <= s.to && s.kind == Kind::UnsupportedBinding)
        })
    {
        properties::REFUSAL
    } else {
        "Spring/MyBatis identity is ambiguous in this source scope"
    }
}

/// Reject collisions/ambiguous declarations before producing any edits.
pub fn plan(sources: &Sources, target: &Target, replacement: &str) -> Result<Plan, &'static str> {
    let all = sites(sources);
    if let Some(property) = &target.property {
        return properties::plan(sources, &all, property, replacement);
    }
    let (old, new) = match &target.identity {
        Identity::Bean(old) => {
            if !simple_name(replacement) {
                return Err("Invalid bean name");
            }
            let class = target.java.as_ref().is_some_and(|j| {
                all.iter().any(|s| {
                    s.path == j.path && matches!(s.kind, Kind::ImplicitBean { class: true, .. }) && s.name == j.old_name
                })
            });
            (old.as_str(), if class { spring_text::decapitalize(replacement) } else { replacement.to_owned() })
        }
        Identity::Config(old) => {
            if properties::unresolved_prefix(sources) {
                return Err(properties::REFUSAL);
            }
            if !config_name(replacement) {
                return Err("Invalid configuration key");
            }
            // A mapping key can change within its current parent. Moving YAML nodes is a different refactoring.
            if old.rsplit_once('.').map(|(p, _)| p) != replacement.rsplit_once('.').map(|(p, _)| p) {
                return Err("Configuration key must stay in the same parent mapping");
            }
            let prefixes: Vec<_> = all.iter().filter(|s| s.kind == Kind::Prefix).collect();
            if prefixes.iter().filter(|p| p.name == *old || p.name.starts_with(&format!("{old}."))).any(|p| {
                let renamed = format!("{replacement}{}", &p.name[old.len()..]);
                renamed.split('.').any(|segment| {
                    !segment.starts_with(|c: char| c.is_ascii_lowercase())
                        || segment.ends_with('-')
                        || !segment.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
                })
            }) {
                return Err("Invalid configuration key");
            }
            if prefixes.iter().any(|p| properties::under_prefix(old, &p.name)) {
                return Err(properties::REFUSAL);
            }
            (old.as_str(), replacement.to_owned())
        }
        Identity::Statement { namespace, name } => {
            if !java_name(replacement) {
                return Err("Invalid mapper method name");
            }
            let methods: Vec<_> =
                all.iter().filter(|s| s.name == *name && s.kind == Kind::JavaStatement(namespace.clone())).collect();
            let definitions: Vec<_> =
                all.iter().filter(|s| s.name == *name && s.kind == Kind::StatementDef(namespace.clone())).collect();
            // Multiple databaseId variants and overloaded methods need runtime/database knowledge.
            if methods.len() != 1 || definitions.len() != 1 {
                return Err("Mapper statement is ambiguous or has no unique Java method");
            }
            (name.as_str(), replacement.to_owned())
        }
    };
    if old == new {
        return Ok(Plan::default());
    }
    let matches_name = |name: &str| match target.identity {
        Identity::Config(_) => name == old || name.starts_with(&format!("{old}.")),
        _ => name == old,
    };
    let clashes = |name: &str| match target.identity {
        Identity::Config(_) => name == new || name.starts_with(&format!("{new}.")),
        _ => name == new,
    };
    if all.iter().any(|s| same_family(&s.kind, &target.identity) && clashes(&s.name) && !matches_name(&s.name))
        || matches!(target.identity, Identity::Bean(_))
            && all.iter().any(|s| matches!(&s.kind, Kind::ImplicitBean { bean, .. } if bean == &new))
    {
        return Err("The target identity already exists in this source scope");
    }
    if target.java.is_some() && !java_name(replacement) {
        return Err("Invalid Java identifier");
    }
    let mut out = Plan { java: target.java.clone(), ..Default::default() };
    if let Identity::Bean(old) = &target.identity {
        if all.iter().any(|s| s.name == new && s.kind == Kind::QualifierTag) {
            return Err("The target identity already exists in this source scope");
        }
        // A rename started at a name reference changes bean identity only.
        // Materialize the name without changing its Java class/factory method.
        if target.java.is_none()
            && let Some(site) = all.iter().find(|s| matches!(&s.kind, Kind::ImplicitBean { bean, .. } if bean == old))
        {
            let text = &sources[&site.path];
            let file = java::parse(text);
            let annotation = file
                .types
                .iter()
                .find_map(|t| {
                    if t.at == site.from {
                        t.annotations.iter().find(|a| {
                            a.args.is_empty()
                                && ["Component", "Service", "Repository", "Controller"].contains(&a.name.as_str())
                                && annotation_is(&file, text, a, &format!("org.springframework.stereotype.{}", a.name))
                        })
                    } else {
                        t.methods().find(|m| m.at == site.from).and_then(|m| {
                            m.annotations.iter().find(|a| {
                                a.args.is_empty()
                                    && annotation_is(&file, text, a, "org.springframework.context.annotation.Bean")
                            })
                        })
                    }
                })
                .ok_or("Implicit bean declaration could not be identified")?;
            let name_end = annotation.name_at + annotation.name.len();
            let (at, value) = match text[name_end..annotation.end].find('(') {
                Some(open) => (name_end + open + 1, format!("\"{new}\"")),
                None => (annotation.end, format!("(\"{new}\")")),
            };
            out.files
                .entry(site.path.clone())
                .or_default()
                .push(TextEdit { range: spring_text::range(text, at, at), text: value });
        }
    }
    if let Identity::Statement { namespace, .. } = &target.identity {
        let method = all.iter().find(|s| s.name == old && s.kind == Kind::JavaStatement(namespace.clone())).unwrap();
        out.java = Some(JavaRename {
            path: method.path.clone(),
            pos: openidea_plugin::text::position(&sources[&method.path], method.from),
            old_name: old.to_owned(),
            new_name: None,
        });
    }
    for s in &all {
        if !same_family(&s.kind, &target.identity) || !matches_name(&s.name) || matches!(s.kind, Kind::JavaStatement(_))
        {
            continue;
        }
        let text = &sources[&s.path];
        let raw = &text[s.from..s.to];
        let replacement = if let Identity::Config(_) = target.identity {
            if s.kind == Kind::ConfigDef && raw != s.name {
                // A YAML parent rename updates all its descendants without editing each leaf.
                if s.name != old {
                    continue;
                }
                new.rsplit('.').next().unwrap().to_owned()
            } else {
                format!("{new}{}", &s.name[old.len()..])
            }
        } else {
            new.clone()
        };
        out.files
            .entry(s.path.clone())
            .or_default()
            .push(TextEdit { range: spring_text::range(text, s.from, s.to), text: replacement });
    }
    Ok(out)
}

#[cfg(test)]
mod tests;
