//! Mapper intentions. Discovery reads only the supplied snapshot; resolution does
//! project I/O and returns edits to the host's preview/undo/version-check pipeline.

use openidea_plugin::providers::{CodeAction, Document};
use openidea_plugin::text::{offset, position};
use openidea_plugin::{Error, Project, Range, TextEdit, host};
use std::collections::{BTreeMap, BTreeSet};

const MAX_SOURCE: usize = 256 * 1024;
const MAX_DISCOVERY: usize = 64 * 1024;
const MAX_XML_BYTES: usize = 2 * 1024 * 1024;
const MAX_XML_FILES: u32 = 1000;
const ACTION: &str = "mybatis.generate-xml.v1";

#[derive(Clone, Debug)]
struct Token<'a> {
    value: &'a str,
    start: usize,
    end: usize,
}

// A small lexer rather than splitting declarations on commas/semicolons: both
// can occur in annotations, literals and comments. Offsets always remain bytes.
fn tokens(text: &str) -> Option<Vec<Token<'_>>> {
    let mut out = Vec::new();
    let mut at = 0;
    while at < text.len() {
        let rest = &text[at..];
        let c = rest.chars().next()?;
        if c.is_whitespace() {
            at += c.len_utf8();
            continue;
        }
        if rest.starts_with("//") {
            at += rest.find('\n').unwrap_or(rest.len());
            continue;
        }
        if rest.starts_with("/*") {
            at += rest.find("*/")? + 2;
            continue;
        }
        let start = at;
        if c == '"' || c == '\'' {
            at += 1;
            loop {
                let d = text[at..].chars().next()?;
                at += d.len_utf8();
                if d == '\\' {
                    at += text[at..].chars().next()?.len_utf8();
                } else if d == c {
                    break;
                }
            }
        } else if super::ident(c) {
            at += rest.chars().take_while(|c| super::ident(*c)).map(char::len_utf8).sum::<usize>();
        } else {
            at += c.len_utf8();
        }
        out.push(Token { value: &text[start..at], start, end: at });
    }
    Some(out)
}

fn matching(ts: &[Token<'_>], start: usize, open: &str, close: &str) -> Option<usize> {
    let mut depth = 0usize;
    for (i, t) in ts.iter().enumerate().skip(start) {
        if t.value == open {
            depth += 1;
        } else if t.value == close {
            depth = depth.checked_sub(1)?;
            if depth == 0 {
                return Some(i);
            }
        }
    }
    None
}

#[derive(Default)]
struct Annotations {
    names: Vec<String>,
    param: Option<String>,
}

fn unannotated<'a>(ts: &[Token<'a>]) -> Option<(Vec<Token<'a>>, Annotations)> {
    let mut clean = Vec::new();
    let mut annotations = Annotations::default();
    let mut at = 0;
    while at < ts.len() {
        if ts[at].value != "@" {
            clean.push(ts[at].clone());
            at += 1;
            continue;
        }
        at += 1;
        let mut name = ts.get(at)?.value;
        at += 1;
        while ts.get(at).is_some_and(|t| t.value == ".") {
            name = ts.get(at + 1)?.value;
            at += 2;
        }
        annotations.names.push(name.to_owned());
        if ts.get(at).is_some_and(|t| t.value == "(") {
            let end = matching(ts, at, "(", ")")?;
            if name == "Param" {
                let args = &ts[at + 1..end];
                let literal = match args {
                    [value] => value.value,
                    [key, eq, value] if key.value == "value" && eq.value == "=" => value.value,
                    _ => return None,
                };
                let value = literal.strip_prefix('"')?.strip_suffix('"')?;
                if !java_name(value) {
                    return None;
                }
                annotations.param = Some(value.to_owned());
            }
            at = end + 1;
        }
    }
    Some((clean, annotations))
}

fn java_name(s: &str) -> bool {
    !s.is_empty() && s.chars().all(super::ident) && !s.starts_with(|c: char| c.is_ascii_digit())
}

fn joined(ts: &[Token<'_>]) -> String {
    ts.iter().map(|t| t.value).collect()
}

#[derive(Clone, Debug)]
struct Parameter {
    name: String,
    ty: String,
    alias: Option<String>,
}

#[derive(Debug)]
struct Method {
    name: String,
    start: usize,
    end: usize,
    name_start: usize,
    result: String,
    parameters: Vec<Parameter>,
}

#[derive(Debug)]
struct Mapper {
    namespace: String,
    package: String,
    imports: BTreeMap<String, String>,
    wildcard: bool,
    methods: Vec<Method>,
    declarations: BTreeMap<String, usize>,
}

fn declaration_name(ts: &[Token<'_>]) -> Option<String> {
    let (clean, _) = unannotated(ts)?;
    let paren = clean.iter().position(|t| t.value == "(")?;
    if clean[..paren].iter().any(|t| t.value == "=") {
        return None;
    }
    let name = clean.get(paren.checked_sub(1)?)?.value;
    java_name(name).then(|| name.to_owned())
}

fn parameters(ts: &[Token<'_>]) -> Option<Vec<Parameter>> {
    if ts.is_empty() {
        return Some(Vec::new());
    }
    let mut parts = Vec::new();
    let (mut start, mut generic, mut parens) = (0, 0i32, 0i32);
    for (i, t) in ts.iter().enumerate() {
        match t.value {
            "<" => generic += 1,
            ">" => generic -= 1,
            "(" => parens += 1,
            ")" => parens -= 1,
            "," if generic == 0 && parens == 0 => {
                parts.push(&ts[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    if generic != 0 || parens != 0 {
        return None;
    }
    parts.push(&ts[start..]);
    parts
        .into_iter()
        .map(|part| {
            let (mut clean, annotation) = unannotated(part)?;
            clean.retain(|t| t.value != "final");
            // Java also permits `User users[]`.
            let mut array = String::new();
            while clean.last().is_some_and(|t| t.value == "]") {
                clean.pop();
                if clean.pop()?.value != "[" {
                    return None;
                }
                array.push_str("[]");
            }
            let name = clean.pop()?.value;
            let ty = joined(&clean).replace("...", "[]") + &array;
            (java_name(name) && !ty.is_empty()).then(|| Parameter { name: name.into(), ty, alias: annotation.param })
        })
        .collect()
}

fn method(ts: &[Token<'_>]) -> Option<Method> {
    let (clean, annotations) = unannotated(ts)?;
    if annotations.names.iter().any(|a| {
        matches!(
            a.as_str(),
            "Select"
                | "Insert"
                | "Update"
                | "Delete"
                | "SelectProvider"
                | "InsertProvider"
                | "UpdateProvider"
                | "DeleteProvider"
        )
    }) || clean.iter().any(|t| matches!(t.value, "=" | "default" | "static" | "private"))
    {
        return None;
    }
    let paren = clean.iter().position(|t| t.value == "(")?;
    let original_paren = ts.iter().position(|t| t.start == clean[paren].start)?;
    let original_end = matching(ts, original_paren, "(", ")")?;
    let name = clean.get(paren.checked_sub(1)?)?;
    let result: Vec<_> =
        clean[..paren - 1].iter().filter(|t| !matches!(t.value, "public" | "abstract")).cloned().collect();
    let result = joined(&result);
    if !java_name(name.value) || result.is_empty() || result.starts_with('<') {
        return None;
    }
    Some(Method {
        name: name.value.into(),
        start: ts.first()?.start,
        end: ts.last()?.end,
        name_start: name.start,
        result,
        parameters: parameters(&ts[original_paren + 1..original_end])?,
    })
}

fn parse_mapper(text: &str) -> Option<Mapper> {
    if text.len() > MAX_SOURCE {
        return None;
    }
    let ts = tokens(text)?;
    let interface = ts.iter().position(|t| t.value == "interface")?;
    // Do not mistake an annotation declaration for a mapper.
    if interface > 0 && ts[interface - 1].value == "@" {
        return None;
    }
    let simple = ts.get(interface + 1)?.value;
    if !java_name(simple) {
        return None;
    }
    let mut package = String::new();
    let mut imports = BTreeMap::new();
    let mut wildcard = false;
    for (i, t) in ts[..interface].iter().enumerate() {
        if matches!(t.value, "package" | "import") {
            let end = ts[i..interface].iter().position(|t| t.value == ";")? + i;
            let value = joined(&ts[i + 1..end]);
            if t.value == "package" {
                package = value;
            } else if value.ends_with(".*") {
                wildcard = true;
            } else {
                imports.insert(value.rsplit('.').next()?.into(), value);
            }
        }
    }
    let body = ts[interface..].iter().position(|t| t.value == "{")? + interface;
    let body_end = matching(&ts, body, "{", "}")?;
    let mut start = body + 1;
    let mut at = start;
    let mut methods = Vec::new();
    let mut declarations = BTreeMap::new();
    while at < body_end {
        match ts[at].value {
            "(" => at = matching(&ts, at, "(", ")")?,
            "{" => {
                if let Some(name) = declaration_name(&ts[start..at]) {
                    *declarations.entry(name).or_insert(0) += 1;
                }
                at = matching(&ts, at, "{", "}")?;
                start = at + 1;
            }
            ";" => {
                if let Some(name) = declaration_name(&ts[start..=at]) {
                    *declarations.entry(name).or_insert(0) += 1;
                }
                if let Some(m) = method(&ts[start..=at]) {
                    methods.push(m);
                }
                start = at + 1;
            }
            _ => {}
        }
        at += 1;
    }
    let namespace = if package.is_empty() { simple.into() } else { format!("{package}.{simple}") };
    Some(Mapper { namespace, package, imports, wildcard, methods, declarations })
}

fn fingerprint(text: &str) -> u64 {
    // Stable across plugin instances; data is opaque, not a security token.
    text.bytes().fold(0xcbf29ce484222325, |h, b| (h ^ u64::from(b)).wrapping_mul(0x100000001b3))
}

pub(super) fn actions(doc: &Document, selection: Range) -> Vec<CodeAction> {
    if doc.language != "java" || doc.text.len() > MAX_DISCOVERY {
        return Vec::new();
    }
    let Some(start) = offset(&doc.text, selection.start) else {
        return Vec::new();
    };
    let Some(end) = offset(&doc.text, selection.end) else {
        return Vec::new();
    };
    let Some(mapper) = parse_mapper(&doc.text) else {
        return Vec::new();
    };
    let Some(m) = mapper.methods.iter().find(|m| start <= m.end && end >= m.start) else {
        return Vec::new();
    };
    if mapper.declarations.get(&m.name) != Some(&1) {
        return Vec::new();
    }
    vec![CodeAction {
        title: format!("Generate MyBatis XML statement for {}", m.name),
        kind: Some("refactor.rewrite".into()),
        preferred: false,
        edits: Vec::new(),
        data: Some(format!(
            "{ACTION}\n{}\n{}\n{}\n{}\n{}",
            doc.version,
            fingerprint(&doc.text),
            m.name_start,
            m.name,
            doc.path
        )),
    }]
}

fn invalid(message: &str) -> Error {
    Error::Invalid(message.into())
}

fn check_cancel(task: &host::Task) -> Result<(), Error> {
    if task.is_cancelled() { Err(Error::Cancelled) } else { Ok(()) }
}

pub(super) fn resolve(doc: &Document, mut action: CodeAction) -> Result<CodeAction, Error> {
    let data = action.data.as_deref().ok_or_else(|| invalid("Missing MyBatis action reference"))?;
    let parts: Vec<_> = data.splitn(6, '\n').collect();
    if parts.len() != 6 || parts[0] != ACTION {
        return Err(invalid("Unknown MyBatis action reference"));
    }
    if doc.language != "java"
        || parts[1].parse::<u64>() != Ok(doc.version)
        || parts[2].parse::<u64>() != Ok(fingerprint(&doc.text))
        || parts[5] != doc.path
    {
        return Err(Error::Stale);
    }
    let mapper = parse_mapper(&doc.text).ok_or_else(|| invalid("Cannot parse Mapper interface"))?;
    let m = mapper
        .methods
        .iter()
        .find(|m| m.name == parts[4] && parts[3].parse::<usize>() == Ok(m.name_start))
        .ok_or(Error::Stale)?;
    if mapper.declarations.get(&m.name) != Some(&1) {
        return Err(invalid("Overloaded Mapper methods cannot have distinct XML ids"));
    }
    let project = super::project_of(&doc.path).ok_or_else(|| invalid("Mapper is outside an open project"))?;
    let task = host::begin_task("Generate MyBatis XML statement", true);
    check_cancel(&task)?;
    let paths = project.find_files("**/*.xml", MAX_XML_FILES + 1)?;
    if paths.len() > MAX_XML_FILES as usize {
        return Err(invalid("Too many XML files; narrow the open project before generating"));
    }
    let mut target = None;
    for path in paths {
        check_cancel(&task)?;
        let text = project.read_file(&path)?;
        if !text.contains("<mapper") || xml_namespace(&text).as_deref() != Some(&mapper.namespace) {
            continue;
        }
        if text.len() > MAX_XML_BYTES {
            return Err(invalid("Mapper XML exceeds the 2 MiB generation limit"));
        }
        let xml = xml_mapper(&text)?;
        if xml.namespace != mapper.namespace {
            return Err(invalid("Mapper XML namespace changed while parsing"));
        }
        if target.is_some() {
            return Err(invalid("Multiple XML files use this namespace; choose a unique mapping first"));
        }
        target = Some((path, text, xml));
    }
    let (path, text, xml) =
        target.ok_or_else(|| invalid("No well-formed existing Mapper XML with this namespace; create it first"))?;
    if xml.ids.contains(&m.name) {
        return Err(invalid("This Mapper method already has an XML statement"));
    }
    let bindings = bindings(&project, &mapper, m, &task)?;
    let statement = statement(m, &mapper, &project, &bindings);
    let edit = insertion(&text, &xml, &statement);
    check_cancel(&task)?;
    // read_file includes editor buffers. Catch changes during scanning before
    // the host captures workspace-edit preconditions and applies its own checks.
    if project.read_file(&path)? != text || project.read_file(&doc.path)? != doc.text {
        return Err(Error::Stale);
    }
    action.edits = vec![(path, vec![edit])];
    action.data = None;
    Ok(action)
}

#[derive(Debug)]
struct XmlMapper {
    namespace: String,
    ids: BTreeSet<String>,
    close: usize,
    child: Option<usize>,
}

fn xml_namespace(text: &str) -> Option<String> {
    use quick_xml::{Reader, XmlVersion, events::Event};
    let mut reader = Reader::from_str(text);
    loop {
        match reader.read_event().ok()? {
            Event::Start(tag) | Event::Empty(tag) => {
                if tag.name().as_ref() != "mapper" {
                    return None;
                }
                for attr in tag.attributes() {
                    let attr = attr.ok()?;
                    if attr.key.as_ref() == "namespace" {
                        return Some(attr.normalized_value(XmlVersion::Implicit1_0).ok()?.into_owned());
                    }
                }
                return None;
            }
            Event::Eof => return None,
            _ => {}
        }
    }
}

// Parse structure without expanding entities or fetching the MyBatis DTD. Keep
// original offsets so even comments, CDATA and formatting remain byte-for-byte.
fn xml_mapper(text: &str) -> Result<XmlMapper, Error> {
    use quick_xml::{Reader, events::Event};
    let fail = || invalid("Malformed or unsupported Mapper XML");
    let mut reader = Reader::from_str(text);
    reader.config_mut().check_comments = true;
    let (mut depth, mut root_seen) = (0usize, false);
    let (mut namespace, mut close, mut child) = (None, None, None);
    let mut ids = BTreeSet::new();
    loop {
        let at = reader.buffer_position() as usize;
        let event = reader.read_event().map_err(|_| fail())?;
        match event {
            Event::Start(ref tag) | Event::Empty(ref tag) => {
                let empty = matches!(event, Event::Empty(_));
                let name = tag.name();
                let mut attrs = BTreeMap::new();
                for attr in tag.attributes() {
                    let attr = attr.map_err(|_| fail())?;
                    if attr.value.contains('<') {
                        return Err(fail());
                    }
                    let value = attr.normalized_value(quick_xml::XmlVersion::Implicit1_0).map_err(|_| fail())?;
                    attrs.insert(attr.key.as_ref().to_owned(), value.into_owned());
                }
                if depth == 0 {
                    if root_seen || name.as_ref() != "mapper" || empty {
                        return Err(fail());
                    }
                    root_seen = true;
                    namespace = attrs.remove("namespace");
                } else if depth == 1 {
                    child.get_or_insert(at);
                    if ["select", "insert", "update", "delete"].contains(&name.as_ref())
                        && let Some(id) = attrs.remove("id")
                    {
                        ids.insert(id);
                    }
                }
                if !empty {
                    depth += 1;
                }
            }
            Event::End(_) => {
                depth = depth.checked_sub(1).ok_or_else(fail)?;
                if depth == 0 {
                    close = Some(at);
                }
            }
            Event::Text(t) if depth == 0 => {
                if !t.as_ref().trim().trim_start_matches('\u{feff}').is_empty() {
                    return Err(fail());
                }
            }
            Event::CData(_) if depth == 0 => return Err(fail()),
            Event::DocType(t) => {
                if root_seen || t.as_ref().contains('[') {
                    return Err(fail());
                }
            }
            Event::GeneralRef(r) => {
                if depth == 0
                    || (r.resolve_char_ref().map_err(|_| fail())?.is_none()
                        && !["amp", "lt", "gt", "quot", "apos"].contains(&r.as_ref()))
                {
                    return Err(fail());
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if depth != 0 {
        return Err(fail());
    }
    Ok(XmlMapper { namespace: namespace.ok_or_else(fail)?, ids, close: close.ok_or_else(fail)?, child })
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&apos;")
}

fn type_name(ty: &str, mapper: &Mapper, project: &Project) -> Option<String> {
    let ty = ty.trim();
    if let Some(import) = mapper.imports.get(ty) {
        return Some(import.clone());
    }
    let primitive = match ty {
        "byte" | "Byte" => Some("java.lang.Byte"),
        "short" | "Short" => Some("java.lang.Short"),
        "int" | "Integer" => Some("java.lang.Integer"),
        "long" | "Long" => Some("java.lang.Long"),
        "float" | "Float" => Some("java.lang.Float"),
        "double" | "Double" => Some("java.lang.Double"),
        "boolean" | "Boolean" => Some("java.lang.Boolean"),
        "char" | "Character" => Some("java.lang.Character"),
        "String" => Some("java.lang.String"),
        "Object" => Some("java.lang.Object"),
        _ => None,
    };
    if let Some(p) = primitive {
        return Some(p.into());
    }
    if ty.contains(['<', '>', '?', '[']) || ty == "void" {
        return None;
    }
    if ty.contains('.') {
        return Some(ty.into());
    }
    let local = if mapper.package.is_empty() { ty.into() } else { format!("{}.{}", mapper.package, ty) };
    let glob = format!("**/{}.java", local.replace('.', "/"));
    let exists = !project.find_files(&glob, 1).ok()?.is_empty();
    if !exists && mapper.wildcard {
        return None;
    }
    // Without wildcard imports a non-java.lang unqualified type is in the
    // mapper's package (type parameters are rejected if no class is found).
    exists.then_some(local)
}

fn element_type(ty: &str) -> Option<&str> {
    if let Some(inner) = ty.strip_suffix("[]") {
        return Some(inner);
    }
    let (outer, rest) = ty.split_once('<')?;
    if !matches!(outer.rsplit('.').next()?, "List" | "Collection" | "Iterable" | "Set" | "Optional" | "Cursor") {
        return None;
    }
    let inner = rest.strip_suffix('>')?;
    (!inner.contains([',', '?'])).then_some(inner)
}

fn scalar(ty: &str) -> bool {
    matches!(
        ty.rsplit('.').next().unwrap_or(ty),
        "byte"
            | "Byte"
            | "short"
            | "Short"
            | "int"
            | "Integer"
            | "long"
            | "Long"
            | "float"
            | "Float"
            | "double"
            | "Double"
            | "boolean"
            | "Boolean"
            | "char"
            | "Character"
            | "String"
            | "BigDecimal"
            | "BigInteger"
            | "Date"
            | "LocalDate"
            | "LocalDateTime"
            | "LocalTime"
            | "Instant"
            | "UUID"
    )
}

#[derive(Debug)]
struct Binding {
    property: String,
    collection: Option<String>,
}

fn object_fields(project: &Project, ty: &str, mapper: &Mapper) -> Vec<String> {
    let Some(ty) = type_name(ty, mapper, project) else {
        return Vec::new();
    };
    let Ok(paths) = project.find_files(&format!("**/{}.java", ty.replace('.', "/")), 2) else {
        return Vec::new();
    };
    if paths.len() != 1 {
        return Vec::new();
    }
    let Ok(text) = project.read_file(&paths[0]) else {
        return Vec::new();
    };
    if text.len() > MAX_SOURCE {
        return Vec::new();
    }
    let Some(ts) = tokens(&text) else {
        return Vec::new();
    };
    let Some(class) = ts.iter().position(|t| t.value == "class") else {
        return Vec::new();
    };
    let Some(body) = ts[class..].iter().position(|t| t.value == "{").map(|i| class + i) else {
        return Vec::new();
    };
    let Some(end) = matching(&ts, body, "{", "}") else {
        return Vec::new();
    };
    let (mut at, mut start) = (body + 1, body + 1);
    let mut fields = Vec::new();
    while at < end {
        if ts[at].value == "(" {
            let Some(close) = matching(&ts, at, "(", ")") else {
                break;
            };
            at = close;
        } else if ts[at].value == "{" {
            let Some(close) = matching(&ts, at, "{", "}") else {
                break;
            };
            at = close;
            start = at + 1;
        } else if ts[at].value == ";" {
            if let Some((clean, _)) = unannotated(&ts[start..at]) {
                let decl = &clean[..clean.iter().position(|t| t.value == "=").unwrap_or(clean.len())];
                if !decl.iter().any(|t| matches!(t.value, "static" | "transient" | "(" | ","))
                    && let Some(field) = decl.last()
                    && java_name(field.value)
                    && decl.len() >= 2
                {
                    fields.push(field.value.into());
                }
            }
            start = at + 1;
        }
        at += 1;
    }
    fields.truncate(24);
    fields
}

#[derive(Debug)]
struct Bindings {
    values: Vec<Binding>,
    unresolved: Vec<String>,
}

fn bindings(project: &Project, mapper: &Mapper, m: &Method, task: &host::Task) -> Result<Bindings, Error> {
    check_cancel(task)?;
    let mut bindings = Vec::new();
    let mut unresolved = Vec::new();
    let parameters: Vec<_> = m
        .parameters
        .iter()
        .filter(|p| {
            let outer = p.ty.split('<').next().unwrap_or(&p.ty);
            let resolved = mapper.imports.get(outer).map(String::as_str).unwrap_or(outer);
            !matches!(resolved, "org.apache.ibatis.session.RowBounds" | "org.apache.ibatis.session.ResultHandler")
        })
        .collect();
    let aliases: BTreeSet<_> = parameters.iter().filter_map(|p| p.alias.as_deref()).collect();
    for (i, p) in parameters.iter().enumerate() {
        check_cancel(task)?;
        let single = parameters.len() == 1;
        let key = p.alias.clone().unwrap_or_else(|| if single { p.name.clone() } else { format!("param{}", i + 1) });
        if p.alias.is_none() && !single && aliases.contains(key.as_str()) {
            unresolved.push(p.name.clone());
            continue;
        }
        let before = bindings.len();
        if scalar(&p.ty) {
            bindings.push(Binding { property: key, collection: None });
        } else if let Some(element) = element_type(&p.ty) {
            let outer = p.ty.split('<').next().unwrap_or(&p.ty).rsplit('.').next().unwrap_or("");
            if !matches!(outer, "Optional" | "Cursor" | "Iterable") {
                let collection = if p.alias.is_some() || !single {
                    key
                } else if p.ty.ends_with("[]") {
                    "array".into()
                } else if outer == "List" {
                    "list".into()
                } else {
                    "collection".into()
                };
                if scalar(element) {
                    bindings.push(Binding { property: "item".into(), collection: Some(collection) });
                } else {
                    for field in object_fields(project, element, mapper).into_iter().take(1) {
                        bindings
                            .push(Binding { property: format!("item.{field}"), collection: Some(collection.clone()) });
                    }
                }
            }
        } else {
            for field in object_fields(project, &p.ty, mapper) {
                let property = if p.alias.is_some() || !single { format!("{key}.{field}") } else { field };
                bindings.push(Binding { property, collection: None });
            }
        }
        if bindings.len() == before {
            unresolved.push(p.name.clone());
        }
    }
    Ok(Bindings { values: bindings, unresolved })
}

fn statement(m: &Method, mapper: &Mapper, project: &Project, plan: &Bindings) -> String {
    let bindings = &plan.values;
    let name = m.name.to_ascii_lowercase();
    let kind = if ["insert", "add", "save", "create"].iter().any(|s| name.starts_with(s)) {
        "insert"
    } else if ["update", "modify", "set"].iter().any(|s| name.starts_with(s)) {
        "update"
    } else if ["delete", "remove"].iter().any(|s| name.starts_with(s)) {
        "delete"
    } else {
        "select"
    };
    let mut result = String::new();
    if kind == "select" {
        let ty = element_type(&m.result).unwrap_or(&m.result);
        if ty
            .split_once('<')
            .is_some_and(|(outer, _)| type_name(outer, mapper, project).as_deref() == Some("java.util.Map"))
        {
            result = " resultType=\"java.util.Map\"".into();
        } else if let Some(ty) = type_name(ty, mapper, project) {
            result = format!(" resultType=\"{}\"", xml_escape(&ty));
        }
    }
    let mut lines = vec![
        format!("<{kind} id=\"{}\"{result}>", xml_escape(&m.name)),
        "  <!-- TODO: replace TODO_TABLE and TODO_COLUMN names; review SQL and parameter mappings before use. -->"
            .into(),
    ];
    if kind == "select" && result.is_empty() {
        lines
            .push("  <!-- TODO: set resultType or resultMap; the return type could not be resolved safely. -->".into());
    }
    if !plan.unresolved.is_empty() {
        lines.push(format!("  <!-- TODO: parameter properties could not be resolved for {}; add the required #{{property}} bindings. -->", xml_escape(&plan.unresolved.join(", "))));
    }
    let values: Vec<_> = bindings.iter().filter(|b| b.collection.is_none()).collect();
    match kind {
        "insert" => {
            let cols = (1..=values.len().max(1)).map(|i| format!("TODO_COLUMN_{i}")).collect::<Vec<_>>().join(", ");
            let vals =
                values.iter().map(|b| format!("#{{{}}}", xml_escape(&b.property))).collect::<Vec<_>>().join(", ");
            lines.push(format!("  INSERT INTO TODO_TABLE ({cols})"));
            lines.push(format!("  VALUES ({})", if vals.is_empty() { "NULL" } else { &vals }));
        }
        "update" => {
            lines.push("  UPDATE TODO_TABLE".into());
            let sets = values
                .iter()
                .enumerate()
                .map(|(i, b)| format!("TODO_COLUMN_{} = #{{{}}}", i + 1, xml_escape(&b.property)))
                .collect::<Vec<_>>()
                .join(", ");
            lines.push(format!("  SET {}", if sets.is_empty() { "TODO_COLUMN_1 = NULL" } else { &sets }));
            lines.push("  WHERE 1 = 0 /* TODO: choose a restrictive update predicate */".into());
        }
        _ => {
            lines.push(if kind == "delete" {
                "  DELETE FROM TODO_TABLE".into()
            } else if name.starts_with("count") {
                "  SELECT COUNT(*) FROM TODO_TABLE".into()
            } else {
                "  SELECT * FROM TODO_TABLE".into()
            });
            lines.push("  WHERE 1 = 0 /* TODO: replace with the intended predicate */".into());
            for (i, b) in bindings.iter().enumerate() {
                if let Some(collection) = &b.collection {
                    lines.push(format!("  AND TODO_COLUMN_{} IN", i + 1));
                    lines.push(format!(
                        "  <foreach collection=\"{}\" item=\"item\" open=\"(\" separator=\",\" close=\")\">",
                        xml_escape(collection)
                    ));
                    lines.push(format!("    #{{{}}}", xml_escape(&b.property)));
                    lines.push("  </foreach>".into());
                } else {
                    lines.push(format!("  AND TODO_COLUMN_{} = #{{{}}}", i + 1, xml_escape(&b.property)));
                }
            }
        }
    }
    if matches!(kind, "insert" | "update") {
        for b in bindings.iter().filter(|b| b.collection.is_some()) {
            lines.push(format!(
                "  <!-- TODO: batch mapping: foreach collection=\"{}\" item=\"item\", value #{{{}}}. -->",
                xml_escape(b.collection.as_deref().unwrap_or("")),
                xml_escape(&b.property)
            ));
        }
    }
    lines.push(format!("</{kind}>"));
    lines.join("\n")
}

fn indentation(text: &str, at: usize) -> Option<&str> {
    let start = text[..at].rfind('\n').map_or(0, |i| i + 1);
    let prefix = &text[start..at];
    prefix.chars().all(|c| matches!(c, ' ' | '\t')).then_some(prefix)
}

fn insertion(text: &str, xml: &XmlMapper, statement: &str) -> TextEdit {
    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let closing_indent = indentation(text, xml.close).unwrap_or("");
    let indent = xml
        .child
        .and_then(|i| indentation(text, i))
        .filter(|s| s.len() > closing_indent.len())
        .map(str::to_owned)
        .unwrap_or_else(|| format!("{closing_indent}  "));
    let unit = indent.strip_prefix(closing_indent).filter(|s| !s.is_empty()).unwrap_or("  ");
    let rendered = statement
        .lines()
        .map(|line| {
            let spaces = line.len() - line.trim_start_matches(' ').len();
            format!("{indent}{}{}", unit.repeat(spaces / 2), &line[spaces..])
        })
        .collect::<Vec<_>>()
        .join(newline);
    let start = xml.close - closing_indent.len();
    let prefix = if start == 0 || text[..start].ends_with('\n') { "" } else { newline };
    let pos = position(text, start);
    TextEdit { range: Range::new(pos, pos), text: format!("{prefix}{rendered}{newline}") }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Navigator;
    use openidea_plugin::{mock::MockHost, providers::Providers};

    const PATH: &str = "/project/src/main/java/com/acme/UserMapper.java";
    const XML_PATH: &str = "/project/src/main/resources/mapper/UserMapper.xml";
    const XML: &str = "<?xml version=\"1.0\"?>\n<!DOCTYPE mapper PUBLIC \"-//mybatis.org//DTD Mapper 3.0//EN\" \"https://mybatis.org/dtd/mybatis-3-mapper.dtd\">\n<mapper namespace=\"com.acme.UserMapper\">\n    <!-- 😀 existing statements stay intact -->\n    <select id=\"findById\" resultType=\"com.acme.User\">SELECT id FROM users WHERE id = #{id}</select>\n</mapper>\n";
    const USER: &str = "package com.acme; public class User { private Long id; private String name; private static final String TABLE = \"users\"; public Long getId() { return id; } }";

    fn source(declaration: &str) -> String {
        format!(
            "package com.acme;\nimport java.util.List;\nimport org.apache.ibatis.annotations.Param;\npublic interface UserMapper {{\n    {declaration}\n}}\n"
        )
    }

    fn document(text: &str) -> Document {
        Document { path: PATH.into(), language: "java".into(), version: 7, text: text.into() }
    }

    fn setup(text: &str) -> MockHost {
        let host = MockHost::install();
        host.add_file(PATH, text);
        host.add_file(XML_PATH, XML);
        host.add_file("src/main/java/com/acme/User.java", USER);
        host
    }

    fn offer(doc: &Document, name: &str) -> CodeAction {
        let at = position(&doc.text, doc.text.find(&format!("{name}(")).unwrap());
        Navigator::default().code_actions(doc, Range::new(at, at)).into_iter().next().unwrap()
    }

    fn apply(text: &str, edit: &TextEdit) -> String {
        let start = offset(text, edit.range.start).unwrap();
        let end = offset(text, edit.range.end).unwrap();
        format!("{}{}{}", &text[..start], edit.text, &text[end..])
    }

    fn generate(declaration: &str, name: &str) -> String {
        let text = source(declaration);
        let _host = setup(&text);
        let doc = document(&text);
        let action = Navigator::default().resolve_code_action(&doc, offer(&doc, name)).unwrap();
        assert!(action.data.is_none());
        assert_eq!(action.edits.len(), 1);
        assert_eq!(action.edits[0].0, XML_PATH);
        let generated = apply(XML, &action.edits[0].1[0]);
        let xml = xml_mapper(&generated).unwrap();
        assert_eq!(xml.namespace, "com.acme.UserMapper");
        assert!(xml.ids.contains(name));
        assert!(generated.contains(XML.split("</mapper>").next().unwrap()));
        generated
    }

    #[test]
    fn lists_without_host_calls_and_resolves_after_plugin_restart() {
        let host = MockHost::install().with_capabilities(&[]);
        let text = source("List<User> findByName(@Param(\"name\") String name, int limit);");
        let doc = document(&text);
        let action = offer(&doc, "findByName");
        assert!(action.edits.is_empty());
        assert!(host.tasks().is_empty());
        drop(host);
        let _host = setup(&text);
        let action = Navigator::default().resolve_code_action(&doc, action).unwrap();
        let generated = &action.edits[0].1[0].text;
        assert!(generated.contains("resultType=\"com.acme.User\""));
        assert!(generated.contains("#{name}"));
        assert!(generated.contains("#{param2}"), "unannotated multi-parameters use stable MyBatis aliases");
        assert!(!generated.contains("#{limit}"));
        assert!(generated.contains("TODO_TABLE"));
    }

    #[test]
    fn all_crud_templates_and_scalar_return_types() {
        for (declaration, name, expected) in [
            ("long countByName(@Param(\"name\") String name);", "countByName", "SELECT COUNT(*)"),
            ("User getById(Long id);", "getById", "SELECT *"),
            ("int insert(User user);", "insert", "INSERT INTO TODO_TABLE"),
            ("int update(User user);", "update", "UPDATE TODO_TABLE"),
            ("int deleteById(Long id);", "deleteById", "DELETE FROM TODO_TABLE"),
        ] {
            let xml = generate(declaration, name);
            assert!(xml.contains(expected));
            if name.starts_with("count") {
                assert!(xml.contains("resultType=\"java.lang.Long\""));
            }
            if matches!(name, "insert" | "update") {
                assert!(xml.contains("#{id}"));
                assert!(xml.contains("#{name}"));
                assert!(!xml.contains("#{TABLE}"));
                assert!(!xml.contains("#{user.id}"), "a single unannotated bean exposes properties directly");
            }
        }
    }

    #[test]
    fn annotations_comments_literals_and_nested_generic_parameters() {
        let xml = generate(
            "/** emoji 😀 ; fake void wrong(); */\n    @Audit(scope = {@Scope(\";,)\"), @Scope(\"x\")})\n    @Deprecated\n    public abstract List<User> search(@Param(\"name\") final String name, @Param(\"filter\") java.util.Map<String, List<User>> filter);",
            "search",
        );
        assert!(xml.contains("id=\"search\" resultType=\"com.acme.User\""));
        assert!(xml.contains("#{name}"));
        assert!(!xml.contains("#{filter}"), "unknown map keys are not invented");
        let mapper = parse_mapper(&source("void helper(); default int local() { return 1; } int real();")).unwrap();
        assert_eq!(mapper.methods.iter().map(|m| m.name.as_str()).collect::<Vec<_>>(), ["helper", "real"]);
    }

    #[test]
    fn qualified_param_annotations_and_bean_prefixes() {
        let xml =
            generate("int update(@org.apache.ibatis.annotations.Param(\"user\") User user, Long tenant);", "update");
        assert!(xml.contains("#{user.id}"));
        assert!(xml.contains("#{user.name}"));
        assert!(xml.contains("#{param2}"));
        let xml = generate("int insert(@Param(\"user\") User user);", "insert");
        assert!(xml.contains("#{user.name}"));
    }

    #[test]
    fn collections_use_foreach_and_mybatis_aliases() {
        for (declaration, collection, value) in [
            ("List<User> findByIds(List<Long> ids);", "list", "#{item}"),
            ("List<User> findByIds(Long[] ids);", "array", "#{item}"),
            ("List<User> findByIds(@Param(\"ids\") List<Long> ids);", "ids", "#{item}"),
            ("List<User> findByIds(java.util.Set<Long> ids);", "collection", "#{item}"),
            ("List<User> findByIds(List<User> users);", "list", "#{item.id}"),
            ("List<User> findByIds(Long tenant, List<Long> ids);", "param2", "#{item}"),
        ] {
            let xml = generate(declaration, "findByIds");
            assert!(xml.contains(&format!("<foreach collection=\"{collection}\"")));
            assert!(xml.contains(value));
        }
        let xml = generate("int insertBatch(@Param(\"users\") List<User> users);", "insertBatch");
        assert!(xml.contains("TODO: batch mapping"));
    }

    #[test]
    fn duplicate_statement_ids_never_produce_edits_even_with_database_id() {
        let text = source("User findById(Long id);");
        let host = setup(&text);
        let doc = document(&text);
        for xml in [XML.to_owned(), XML.replace("id=\"findById\"", "databaseId='h2' id = 'findBy&#73;d'")] {
            host.add_file(XML_PATH, &xml);
            let result = resolve(&doc, offer(&doc, "findById"));
            assert!(matches!(result, Err(Error::Invalid(s)) if s.contains("already")));
        }
    }

    #[test]
    fn applying_twice_is_rejected_and_host_edits_are_versioned() {
        let text = source("User findNew(Long id);");
        let host = setup(&text);
        let doc = document(&text);
        let original = offer(&doc, "findNew");
        let action = resolve(&doc, original.clone()).unwrap();
        host.open_editor(XML_PATH, "xml", XML, &[]);
        let editor = host.invocation().editor().unwrap();
        let before = editor.document().unwrap();
        editor.apply_edits(before.version, &action.edits[0].1).unwrap();
        assert!(host.editor_text().contains("id=\"findNew\""));
        assert!(matches!(resolve(&doc, original), Err(Error::Invalid(s)) if s.contains("already")));
        assert_eq!(editor.apply_edits(before.version, &action.edits[0].1), Err(Error::Stale));
    }

    #[test]
    fn rejects_stale_source_version_content_path_and_foreign_data() {
        let text = source("User findNew(Long id);");
        let _host = setup(&text);
        let doc = document(&text);
        let action = offer(&doc, "findNew");
        for changed in [
            Document { version: 8, ..doc.clone() },
            Document { text: text.replace("Long id", "String name"), ..doc.clone() },
            Document { path: "/project/Other.java".into(), ..doc.clone() },
        ] {
            assert_eq!(resolve(&changed, action.clone()), Err(Error::Stale));
        }
        let foreign = CodeAction { data: Some("unrelated".into()), ..action };
        assert!(matches!(resolve(&doc, foreign), Err(Error::Invalid(_))));
    }

    #[test]
    fn utf16_ranges_and_unsaved_java_are_used() {
        let disk = source("User old(Long id);");
        let host = setup(&disk);
        let live = source("/* 😀中文 */ User findLive(@Param(\"id\") Long id);");
        host.open_editor(PATH, "java", &live, &[]);
        let doc = document(&live);
        let action = resolve(&doc, offer(&doc, "findLive")).unwrap();
        let xml = apply(XML, &action.edits[0].1[0]);
        assert!(xml.contains("id=\"findLive\""));
        assert!(xml.contains("#{id}"));
        assert!(!xml.contains("id=\"old\""));
    }

    #[test]
    fn unsaved_xml_is_used_and_existing_unsaved_id_is_not_duplicated() {
        let text = source("User findLive(Long id);");
        let host = setup(&text);
        let live = XML.replace("</mapper>", "    <!-- unsaved 😀 -->\n</mapper>");
        host.open_editor(XML_PATH, "xml", &live, &[]);
        let doc = document(&text);
        let action = resolve(&doc, offer(&doc, "findLive")).unwrap();
        let xml = apply(&live, &action.edits[0].1[0]);
        assert!(xml.contains("<!-- unsaved 😀 -->"));
        host.open_editor(XML_PATH, "xml", &xml, &[]);
        assert!(matches!(resolve(&doc, offer(&doc, "findLive")), Err(Error::Invalid(s)) if s.contains("already")));
    }

    #[test]
    fn ignores_commented_and_cdata_fake_xml_and_preserves_crlf_tabs() {
        let text = source("User findLive(Long id);");
        let host = setup(&text);
        let xml = "<mapper namespace = 'com.acme.UserMapper'>\r\n\t<!-- <select id='findLive'/> -->\r\n\t<select id='existing'><![CDATA[ <select id='findLive'/> ]]></select>\r\n</mapper>\r\n";
        host.add_file(XML_PATH, xml);
        let doc = document(&text);
        let action = resolve(&doc, offer(&doc, "findLive")).unwrap();
        let edit = &action.edits[0].1[0];
        assert!(edit.text.starts_with("\t<select"));
        assert!(!edit.text.replace("\r\n", "").contains('\n'));
        assert!(xml_mapper(&apply(xml, edit)).unwrap().ids.contains("findLive"));
    }

    #[test]
    fn inline_xml_and_utf16_insertion_positions() {
        let text = "<mapper namespace='com.acme.UserMapper'><!-- 😀 --></mapper>";
        let xml = xml_mapper(text).unwrap();
        let edit = insertion(text, &xml, "<select id=\"new\" resultType=\"int\">SELECT 1</select>");
        assert_eq!(edit.range.start.character as usize, text[..xml.close].encode_utf16().count());
        let generated = apply(text, &edit);
        assert!(xml_mapper(&generated).unwrap().ids.contains("new"));
    }

    #[test]
    fn malformed_xml_and_ambiguous_namespace_fail_without_writes() {
        let text = source("User findNew(Long id);");
        let host = setup(&text);
        let doc = document(&text);
        for bad in [
            "<mapper namespace='com.acme.UserMapper'><select></mapper>",
            "<mapper namespace='com.acme.UserMapper'/>",
            "<mapper namespace='com.acme.UserMapper'><select id='a' id='a'/></mapper>",
            "<mapper namespace='com.acme.UserMapper'><!-- broken -- comment --></mapper>",
            "<mapper namespace='com.acme.UserMapper'><select id='a'>&missing;</select></mapper>",
            "<!DOCTYPE mapper [<!ENTITY x 'anything'>]><mapper namespace='com.acme.UserMapper'></mapper>",
            "<mapper namespace='com.acme.UserMapper'></mapper><mapper/>",
        ] {
            assert!(xml_mapper(bad).is_err(), "{bad}");
            host.add_file(XML_PATH, bad);
            assert!(resolve(&doc, offer(&doc, "findNew")).is_err());
        }
        host.add_file(XML_PATH, XML);
        host.add_file("other.xml", XML);
        assert!(matches!(resolve(&doc, offer(&doc, "findNew")), Err(Error::Invalid(s)) if s.contains("Multiple")));
        assert!(host.editor_text().is_empty());
    }

    #[test]
    fn unsupported_types_fall_back_to_explicit_todos() {
        let xml = generate("java.util.List<? extends User> search(java.util.Map<String, Object> filter);", "search");
        assert!(!xml.contains("<select id=\"search\" resultType="));
        assert!(xml.contains("TODO: set resultType or resultMap"));
        assert!(xml.contains("TODO: parameter properties could not be resolved"));
        let xml = generate("User[] findAll();", "findAll");
        assert!(xml.contains("resultType=\"com.acme.User\""));
    }

    #[test]
    fn no_action_for_annotated_sql_default_methods_overloads_or_non_java() {
        for declaration in [
            "@Select(\"select 1;\") int count();",
            "@org.apache.ibatis.annotations.SelectProvider(type = Sql.class, method = \"sql\") int count();",
            "default int count() { return 1; }",
            "static int count() { return 1; }",
            "int count(); int count(Long id);",
            "int count(); @Select(\"select 1\") int count(Long id);",
            "int count(); default int count(Long id) { return 1; }",
        ] {
            let doc = document(&source(declaration));
            let at = position(&doc.text, doc.text.find("count(").unwrap());
            assert!(actions(&doc, Range::new(at, at)).is_empty(), "{declaration}");
        }
        let mut doc = document(&source("int count();"));
        doc.language = "xml".into();
        assert!(actions(&doc, Range::default()).is_empty());
        let doc = document("public interface Incomplete { int count(");
        assert!(actions(&doc, Range::default()).is_empty());
    }

    #[test]
    fn cancellation_and_capability_denial_are_errors_without_writes() {
        let text = source("int count();");
        let host = setup(&text);
        let task = host::begin_task("test", true);
        host.cancel_tasks();
        assert_eq!(check_cancel(&task), Err(Error::Cancelled));
        let mapper = parse_mapper(&text).unwrap();
        let project = host::projects().unwrap().remove(0);
        assert!(matches!(bindings(&project, &mapper, &mapper.methods[0], &task), Err(Error::Cancelled)));
        drop(task);
        drop(host);
        let _host = MockHost::install().with_capabilities(&[]);
        let doc = document(&text);
        assert!(resolve(&doc, offer(&doc, "count")).is_err());
    }

    #[test]
    fn named_annotation_values_special_parameters_and_alias_collisions() {
        let xml = generate(
            "User findLive(@Param(value = \"name\") String name, org.apache.ibatis.session.RowBounds bounds, Long id);",
            "findLive",
        );
        assert!(xml.contains("#{name}"));
        assert!(xml.contains("#{param2}"));
        assert!(!xml.contains("#{param3}"));
        let xml = generate("User findLive(@Param(\"param2\") String name, Long id);", "findLive");
        assert!(xml.contains("TODO: parameter properties could not be resolved for id"));
        assert_eq!(xml.matches("#{param2}").count(), 1);
        let xml = generate("User findLive(String name, java.util.Map<String, Object> filter);", "findLive");
        assert!(xml.contains("#{param1}"));
        assert!(xml.contains("TODO: parameter properties could not be resolved for filter"));
    }

    #[test]
    fn result_types_use_imports_and_avoid_ambiguous_wildcards() {
        for (imports, result, expected) in [
            ("import other.domain.Account;", "List<Account>", "other.domain.Account"),
            ("import java.util.Map;", "Map<String, Object>", "java.util.Map"),
            ("", "java.util.List<java.lang.String>", "java.lang.String"),
        ] {
            let text = source(&format!("{result} findLive();"))
                .replace("public interface", &format!("{imports}\npublic interface"));
            let _host = setup(&text);
            let doc = document(&text);
            let action = resolve(&doc, offer(&doc, "findLive")).unwrap();
            assert!(action.edits[0].1[0].text.contains(&format!("resultType=\"{expected}\"")));
        }
        let text =
            source("List<Account> findLive();").replace("public interface", "import a.*; import b.*; public interface");
        let _host = setup(&text);
        let doc = document(&text);
        let action = resolve(&doc, offer(&doc, "findLive")).unwrap();
        assert!(action.edits[0].1[0].text.contains("TODO: set resultType or resultMap"));
    }

    #[test]
    fn multiline_selection_and_source_size_bound() {
        let text = source("List<User> search(\n @Param(\"name\") String name,\n Long id\n );");
        let doc = document(&text);
        let at = position(&text, text.find("String name").unwrap());
        assert_eq!(actions(&doc, Range::new(at, at)).len(), 1);
        let doc = document(&" ".repeat(MAX_DISCOVERY + 1));
        assert!(actions(&doc, Range::default()).is_empty());
    }

    #[test]
    fn snapshot_discovery_budget_sample() {
        let _host = MockHost::install().with_capabilities(&[]);
        let methods = (0..800).map(|i| format!("User findById{i}(@Param(\"id\") Long id);")).collect::<String>();
        let doc = document(&source(&methods));
        let at = position(&doc.text, doc.text.find("findById799(").unwrap());
        let started = std::time::Instant::now();
        assert_eq!(actions(&doc, Range::new(at, at)).len(), 1);
        println!("snapshot discovery: {} bytes, 800 methods, {:?}", doc.text.len(), started.elapsed());
    }

    // Exercises the shared Java 8 project without touching any of its files.
    // The integration session owns real editor/plugin installation checks.
    #[test]
    #[ignore = "set OPENIDEA_MYBATIS_JAVA8_FIXTURE to the shared Java 8 project"]
    fn java8_project_read_only() {
        let root = std::path::PathBuf::from(std::env::var("OPENIDEA_MYBATIS_JAVA8_FIXTURE").unwrap());
        let java_path = root.join("src/main/java/com/example/demo/UserMapper.java");
        let xml_path = root.join("src/main/resources/mapper/UserMapper.xml");
        let entity_path = root.join("src/main/java/com/example/demo/User.java");
        let original_java = std::fs::read_to_string(&java_path).unwrap();
        let original_xml = std::fs::read_to_string(&xml_path).unwrap();
        let entity = std::fs::read_to_string(&entity_path).unwrap();
        for (name, declaration) in [
            ("findGenerated", "java.util.List<User> findGenerated(@Param(\"name\") String name, Long limit);"),
            ("countGenerated", "long countGenerated(@Param(\"name\") String name);"),
            ("insertGenerated", "int insertGenerated(User user);"),
            ("updateGenerated", "int updateGenerated(@Param(\"user\") User user);"),
            ("deleteGenerated", "int deleteGenerated(@Param(\"id\") Long id);"),
        ] {
            let close = original_java.rfind('}').unwrap();
            let live = format!("{}    {declaration}\n{}", &original_java[..close], &original_java[close..]);
            let host = MockHost::install();
            let path = "/project/src/main/java/com/example/demo/UserMapper.java";
            host.open_editor(path, "java", &live, &[]);
            host.add_file("src/main/java/com/example/demo/User.java", &entity);
            host.add_file(XML_PATH, &original_xml);
            let doc = Document { path: path.into(), text: live, ..document("") };
            let action = resolve(&doc, offer(&doc, name)).unwrap();
            let generated = apply(&original_xml, &action.edits[0].1[0]);
            let parsed = xml_mapper(&generated).unwrap();
            assert_eq!(parsed.namespace, "com.example.demo.UserMapper");
            assert!(parsed.ids.contains(name));
            assert!(parsed.ids.contains("findById"));
            assert!(parsed.ids.contains("findByName"));
            if name == "findGenerated" {
                assert!(generated.contains("resultType=\"com.example.demo.User\""));
                assert!(generated.contains("#{param2}"));
            }
            if name == "insertGenerated" {
                assert!(generated.contains("#{name}"));
            }
            if name == "updateGenerated" {
                assert!(generated.contains("#{user.name}"));
            }
            host.open_editor(XML_PATH, "xml", &generated, &[]);
            host.add_file(path, &doc.text);
            assert!(matches!(resolve(&doc, offer(&doc, name)), Err(Error::Invalid(s)) if s.contains("already")));
        }
        assert_eq!(std::fs::read_to_string(&java_path).unwrap(), original_java);
        assert_eq!(std::fs::read_to_string(&xml_path).unwrap(), original_xml);
    }
}
