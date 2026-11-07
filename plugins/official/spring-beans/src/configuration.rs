//! Navigation adds source provenance and placeholder chains; Spring Tools keeps completion/validation.
use crate::{FileModel, environment};
use openidea_plugin::providers::Document;
use openidea_plugin::views::{Body, Button, TreeItem, View};
use openidea_plugin::{Location, Position, Range, host};
use spring_text::java::{self, Annotation, JavaFile, MemberKind, TypeRef};
use spring_text::{config, contains, range};
use std::collections::{BTreeMap, BTreeSet};

pub const VIEW: &str = "configuration-sources";
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Model {
    groups: Vec<Group>,
    fields: BTreeMap<String, Vec<Field>>,
    references: Vec<Reference>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Group {
    prefix: String,
    ty: String,
    range: Range,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Field {
    name: String,
    ty: String,
    range: Range,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reference {
    pub key: String,
    pub range: Range,
}
#[derive(Clone)]
pub struct Binding {
    pub key: String,
    pub location: Location,
    pub prefix: bool,
}
fn spring(file: &JavaFile, text: &str, a: &Annotation) -> bool {
    let qualifier = &text[a.at + 1..a.name_at];
    (qualifier.is_empty() || qualifier.starts_with("org.springframework.")) && file.from_spring(&a.name)
}
fn qualified(file: &JavaFile, ty: &TypeRef) -> String {
    let head = ty.text.split('<').next().unwrap_or(&ty.name).trim();
    if head.contains('.') {
        return head.into();
    }
    if let Some(import) = file.imports.iter().find(|i| !i.is_static && i.path.ends_with(&format!(".{}", ty.name))) {
        return import.path.clone();
    }
    if let Some((i, _)) = file.types.iter().enumerate().find(|(_, t)| t.name == ty.name) {
        return file.qualified(i);
    }
    if file.package.is_empty() { ty.name.clone() } else { format!("{}.{}", file.package, ty.name) }
}
fn kebab(name: &str) -> String {
    let mut out = String::new();
    let mut previous_lower = false;
    for c in name.chars() {
        if c.is_uppercase() && previous_lower {
            out.push('-');
        }
        out.extend(c.to_lowercase());
        previous_lower = c.is_lowercase() || c.is_numeric();
    }
    out
}
fn group(file: &JavaFile, text: &str, annotations: &[Annotation], ty: String, at: Range) -> Option<Group> {
    let a = annotations.iter().find(|a| a.name == "ConfigurationProperties" && spring(file, text, a))?;
    if a.args.iter().filter(|a| ["prefix", "value"].contains(&a.name.as_str())).any(|a| {
        let tokens = java::tokens(&a.text);
        tokens.len() != 1 || tokens[0].string().is_none()
    }) {
        return None;
    }
    let prefix = a.strings(&["value", "prefix"]).first().map(|s| s.value.clone()).unwrap_or_default();
    (!prefix.contains(['$', '\\'])).then_some(Group { prefix, ty, range: at })
}
impl Model {
    pub fn java(text: &str, file: &JavaFile) -> Self {
        let mut out = Self::default();
        for (i, ty) in file.types.iter().enumerate() {
            let name = file.qualified(i);
            if let Some(group) =
                group(file, text, &ty.annotations, name.clone(), range(text, ty.at, ty.at + ty.name.len()))
            {
                out.groups.push(group);
            }
            let mut fields = vec![];
            let mut annotations: Vec<&Annotation> =
                ty.annotations.iter().chain(ty.components.iter().flat_map(|p| &p.annotations)).collect();
            for member in &ty.members {
                annotations.extend(&member.annotations);
                annotations.extend(member.params.iter().flat_map(|p| &p.annotations));
                if member.kind == MemberKind::Field
                    && !member.modifiers.iter().any(|m| m == "static")
                    && let Some(kind) = &member.ty
                {
                    fields.push(Field {
                        name: kebab(&member.name),
                        ty: qualified(file, kind),
                        range: range(text, member.at, member.at + member.name.len()),
                    });
                }
                if member.kind == MemberKind::Method
                    && member.has(&["Bean"])
                    && let Some(kind) = &member.ty
                    && let Some(group) = group(
                        file,
                        text,
                        &member.annotations,
                        qualified(file, kind),
                        range(text, member.at, member.at + member.name.len()),
                    )
                {
                    out.groups.push(group);
                }
            }
            out.fields.insert(name, fields);
            for a in annotations.into_iter().filter(|a| spring(file, text, a)) {
                for value in a.args.iter().flat_map(|a| &a.strings) {
                    let raw = &text[value.at..value.end];
                    if raw.contains('\\') {
                        continue;
                    }
                    out.references.extend(environment::references(raw).into_iter().map(|(key, start, end)| {
                        Reference { key, range: range(text, value.at + start, value.at + end) }
                    }));
                }
            }
        }
        out
    }
}
pub fn bindings(files: &BTreeMap<String, FileModel>) -> Vec<Binding> {
    let mut out = vec![];
    fn fields(
        ty: &str,
        prefix: &str,
        files: &BTreeMap<String, FileModel>,
        seen: &mut BTreeSet<String>,
        out: &mut Vec<Binding>,
        depth: usize,
    ) {
        if depth > 8 || out.len() > 4096 || !seen.insert(ty.into()) {
            return;
        }
        let candidates: Vec<_> =
            files.iter().filter_map(|(path, m)| m.configuration.fields.get(ty).map(|fields| (path, fields))).collect();
        if candidates.len() == 1 {
            let (path, items) = candidates[0];
            for field in items {
                let key = if prefix.is_empty() { field.name.clone() } else { format!("{prefix}.{}", field.name) };
                out.push(Binding {
                    key: key.clone(),
                    location: Location { path: path.clone(), range: field.range },
                    prefix: false,
                });
                fields(&field.ty, &key, files, seen, out, depth + 1);
            }
        }
        seen.remove(ty);
    }
    for (path, model) in files {
        for group in &model.configuration.groups {
            out.push(Binding {
                key: group.prefix.clone(),
                location: Location { path: path.clone(), range: group.range },
                prefix: true,
            });
            fields(&group.ty, &group.prefix, files, &mut BTreeSet::new(), &mut out, 0);
        }
    }
    let mut seen = BTreeSet::new();
    out.retain(|b| {
        seen.insert((
            b.key.clone(),
            b.location.path.clone(),
            b.location.range.start.line,
            b.location.range.start.character,
            b.prefix,
        ))
    });
    out
}
pub fn declarations(snapshot: &environment::Snapshot, key: &str) -> Vec<Location> {
    let sources = snapshot.candidates(key);
    if !matches!(snapshot.resolve(key).state, environment::State::Unknown(_))
        && let Some(location) = sources.last().and_then(|s| s.location.clone())
    {
        return vec![location];
    }
    let mut locations: Vec<_> = sources.iter().rev().filter_map(|s| s.location.clone()).collect();
    locations.dedup();
    locations
}
pub fn definition(doc: &Document, at: Position, files: &BTreeMap<String, FileModel>) -> Vec<Location> {
    let Some(cursor) = openidea_plugin::text::offset(&doc.text, at) else { return vec![] };
    let current = [".properties", ".yaml", ".yml"]
        .iter()
        .any(|e| doc.path.ends_with(e))
        .then_some((doc.path.as_str(), doc.text.as_str()));
    let snapshot = environment::snapshot(files, current);
    if doc.language == "java" {
        if let Some(model) = files.get(&doc.path)
            && let Some(reference) = model.configuration.references.iter().find(|r| contains(r.range, at))
        {
            return declarations(&snapshot, &reference.key);
        }
        return bindings(files)
            .into_iter()
            .filter(|b| b.location.path == doc.path && contains(b.location.range, at) && !b.prefix)
            .flat_map(|b| declarations(&snapshot, &b.key))
            .collect();
    }
    if !config::is_config_file(&doc.path) && !snapshot.texts.contains_key(&doc.path) {
        return vec![];
    }
    if let Some((key, _, _)) =
        environment::references(&doc.text).into_iter().find(|(_, start, end)| *start <= cursor && cursor <= *end)
    {
        return declarations(&snapshot, &key);
    }
    if !host::setting_bool("springBeans.configurationBindingFallback").unwrap_or(false) {
        return vec![];
    }
    let Some(key) = config::keys(&doc.path, &doc.text).into_iter().find(|k| k.at <= cursor && cursor <= k.end) else {
        return vec![];
    };
    let canonical = config::canonical(&key.key);
    bindings(files)
        .into_iter()
        .filter(|b| !b.prefix && config::canonical(&b.key) == canonical)
        .map(|b| b.location)
        .collect()
}
fn location_key(parent: &str, role: &str, location: &Location) -> String {
    format!(
        "c:{}",
        serde_json::json!([parent, role, location.path, location.range.start.line, location.range.start.character])
    )
}
pub fn location(key: &str) -> Option<Location> {
    let value: serde_json::Value = serde_json::from_str(key.strip_prefix("c:")?).ok()?;
    let path = value.get(2)?.as_str()?.to_owned();
    let at = Position {
        line: value.get(3)?.as_u64()?.try_into().ok()?,
        character: value.get(4)?.as_u64()?.try_into().ok()?,
    };
    Some(Location { path, range: Range { start: at, end: at } })
}

pub fn view(snapshot: &environment::Snapshot, files: &BTreeMap<String, FileModel>, filter: &str) -> View {
    let mut items = vec![];
    let bindings = bindings(files);
    let mut keys = snapshot.keys();
    keys.extend(bindings.iter().filter(|b| !b.prefix).map(|b| b.key.clone()));
    keys.extend(files.values().flat_map(|m| &m.configuration.references).map(|r| r.key.clone()));
    let filter = filter.to_lowercase();
    let keys: Vec<_> = keys.into_iter().filter(|k| k.to_lowercase().contains(&filter)).collect();
    let truncated = keys.len() > 500;
    for key in keys.into_iter().take(500) {
        let parent = format!("property:{key}");
        let resolution = snapshot.resolve(&key);
        let detail = match &resolution.state {
            environment::State::Value(_) => "Resolved · values hidden".into(),
            environment::State::Missing => "Known absent".into(),
            environment::State::Unknown(reason) => format!("Unknown · {reason}"),
        };
        items.push(TreeItem::new(&parent, &key).detail(&detail).expandable(false));
        for (index, source) in snapshot.candidates(&key).into_iter().rev().enumerate() {
            let id = source
                .location
                .as_ref()
                .map(|l| location_key(&parent, &format!("source:{index}"), l))
                .unwrap_or_else(|| format!("origin:{key}:{index}"));
            items.push(
                TreeItem::new(&id, &source.origin)
                    .parent(&parent)
                    .detail(if matches!(resolution.state, environment::State::Unknown(_)) {
                        "Candidate source"
                    } else if index == 0 {
                        "Selected source"
                    } else {
                        "Overridden source"
                    })
                    .icon("file-properties"),
            );
        }
        for binding in bindings.iter().filter(|b| !b.prefix && config::canonical(&b.key) == config::canonical(&key)) {
            items.push(
                TreeItem::new(&location_key(&parent, "binding", &binding.location), "Java binding")
                    .parent(&parent)
                    .detail(binding.location.path.rsplit('/').next().unwrap_or(""))
                    .icon("property"),
            );
        }
        let mut chain = BTreeSet::new();
        for next in resolution.chain.iter().skip(1).filter(|key| chain.insert(key.as_str())) {
            if let Some(location) = declarations(snapshot, next).first() {
                items.push(
                    TreeItem::new(&location_key(&parent, next, location), &format!("→ {next}"))
                        .parent(&parent)
                        .detail("Placeholder source")
                        .icon("reference"),
                );
            }
        }
    }
    if truncated {
        items.push(TreeItem::new("limit", "Showing the first 500 matching keys; narrow the filter").icon("info"));
    }
    for (index, note) in snapshot.notes.iter().enumerate() {
        items.push(TreeItem::new(&format!("note:{index}"), note).icon("info"));
    }
    for (index, warning) in snapshot.warnings.iter().enumerate() {
        items.push(TreeItem::new(&format!("warning:{index}"), warning).icon("warning"));
    }
    let label = snapshot.launch.as_ref().map_or("Source environment".into(), |l| format!("Launch: {l}"));
    let profiles = snapshot
        .profiles
        .as_ref()
        .map_or("profiles unknown".into(), |_| format!("profiles: {}", snapshot.profile_order.join(",")));
    let application =
        if snapshot.application.is_empty() { String::new() } else { format!(" · {}", snapshot.application) };
    View::new(Body::Tree(items))
        .button(Button::new("filter", "Filter").icon("search"))
        .button(Button::new("refresh", "Refresh").icon("refresh"))
        .status(&format!("{label}{application} · {profiles} · values hidden"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;
    fn setup() -> (MockHost, BTreeMap<String, FileModel>) {
        environment::invalidate();
        let host = MockHost::install();
        host.set_setting("springBeans.environmentComplete", "true");
        host.set_setting("springBeans.activeProfiles", "\"dev\"");
        let text = "package sample;\r\n@ConfigurationProperties(prefix=\"demo\")\r\nclass Settings { private String name; private Nested nested; static class Nested { private String label; } }\r\n";
        host.add_file("src/main/java/sample/Settings.java", text);
        let files = BTreeMap::from([(
            "/project/src/main/java/sample/Settings.java".into(),
            crate::model::parse("Settings.java", text).unwrap(),
        )]);
        (host, files)
    }
    fn doc(path: &str, language: &str, text: &str) -> Document {
        Document { path: path.into(), language: language.into(), text: text.into(), version: 1 }
    }
    fn inside(text: &str, needle: &str) -> Position {
        openidea_plugin::text::position(text, text.find(needle).unwrap() + 1)
    }
    #[test]
    fn java_placeholders_and_nested_bindings_navigate_to_effective_source() {
        let (host, mut files) = setup();
        host.add_file("src/main/resources/application.properties", "demo.name=base\n");
        host.add_file(
            "src/main/resources/application-dev.yml",
            "demo:\n  name: profile\n  nested:\n    label: nested\n",
        );
        let text = "package sample; import org.springframework.beans.factory.annotation.Value; class Client { /* 😀 */ @Value(\"${demo.name}\") String value; }";
        files.insert(
            "/project/src/main/java/sample/Client.java".into(),
            crate::model::parse("Client.java", text).unwrap(),
        );
        let targets = definition(
            &doc("/project/src/main/java/sample/Client.java", "java", text),
            inside(text, "demo.name"),
            &files,
        );
        assert_eq!(targets.len(), 1);
        assert!(targets[0].path.ends_with("application-dev.yml"));
        assert_eq!(targets[0].range.start, Position::new(1, 2));
        let text =
            host.invocation().project().unwrap().read_file("/project/src/main/java/sample/Settings.java").unwrap();
        let targets = definition(
            &doc("/project/src/main/java/sample/Settings.java", "java", &text),
            inside(&text, "label;"),
            &files,
        );
        assert_eq!(targets[0].range.start, Position::new(3, 4));
        assert!(bindings(&files).iter().any(|b| b.key == "demo.nested.label"));
    }
    #[test]
    fn config_key_binding_is_opt_in_and_keeps_utf16_positions() {
        let (host, files) = setup();
        let text = "demo:\r\n  name: \"😀 ${missing:${demo.nested.label}}\"\r\n  nested:\r\n    label: source\r\n";
        let path = "/project/src/main/resources/application-dev.yml";
        host.add_file("src/main/resources/application-dev.yml", text);
        let document = doc(path, "yaml", text);
        assert!(definition(&document, inside(text, "name:"), &files).is_empty());
        host.set_setting("springBeans.configurationBindingFallback", "true");
        let targets = definition(&document, inside(text, "name:"), &files);
        assert_eq!(targets.len(), 1);
        assert!(targets[0].path.ends_with("Settings.java"));
        let targets = definition(&document, inside(text, "demo.nested.label"), &files);
        assert_eq!(targets[0].range.start, Position::new(3, 4));
        let model = Model::java(
            "class X { @Value(\"😀 ${名字:默认}\") String v; }",
            &java::parse("class X { @Value(\"😀 ${名字:默认}\") String v; }"),
        );
        let reference = &model.references[0];
        assert_eq!(reference.key, "名字");
        assert_eq!(reference.range.end.character - reference.range.start.character, 2);
    }
    #[test]
    fn imported_config_uses_current_text_and_view_links_hide_values_and_have_unique_keys() {
        let (host, files) = setup();
        host.add_file("src/main/resources/application.properties","spring.config.import=shared.properties\ndemo.name=base-secret\ndemo.alias=${demo.name}\ndemo.both=${demo.name}/${demo.name}\n");
        host.add_file("src/main/resources/shared.properties", "demo.name=imported-secret\n");
        let path = "/project/src/main/resources/shared.properties";
        let text = "demo.name=unsaved-secret\ndemo.alias=${demo.name}\n";
        let targets = definition(&doc(path, "properties", text), inside(text, "demo.name}"), &files);
        assert!(targets[0].path.ends_with("shared.properties"));
        let snapshot = environment::snapshot(&files, Some((path, text)));
        assert_eq!(snapshot.resolve("demo.alias").state, environment::State::Value("unsaved-secret".into()));
        let shown = view(&snapshot, &files, "");
        let debug = format!("{shown:?}");
        for secret in ["base-secret", "imported-secret", "unsaved-secret"] {
            assert!(!debug.contains(secret));
        }
        let Body::Tree(items) = shown.body else { panic!("tree") };
        let unique: BTreeSet<_> = items.iter().map(|i| &i.key).collect();
        assert_eq!(unique.len(), items.len());
        assert!(items.iter().any(|i| i.detail.as_deref() == Some("Overridden source")));
        let linked = items.iter().find(|i| i.label == "Java binding").unwrap();
        assert!(location(&linked.key).unwrap().path.ends_with("Settings.java"));
    }
    #[test]
    fn nonliteral_prefixes_do_not_produce_false_bindings() {
        let (_host, _) = setup();
        for annotation in ["@ConfigurationProperties(PREFIX)", "@ConfigurationProperties(prefix=\"de\"+\"mo\")"] {
            let text = format!("{annotation} class Props {{String name;}}");
            assert!(Model::java(&text, &java::parse(&text)).groups.is_empty());
        }
        let text = "@ConfigurationProperties class Props {String name;}";
        let model = crate::model::parse("Props.java", text).unwrap();
        assert!(bindings(&BTreeMap::from([("/project/Props.java".into(), model)])).iter().any(|b| b.key == "name"));
    }
}
