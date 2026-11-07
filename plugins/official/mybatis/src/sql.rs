//! Local CREATE TABLE metadata and conservative completion in mapper SQL text.
use openidea_plugin::providers::{CompletionItem, CompletionKind};
use openidea_plugin::{Project, host};
use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

const MAX_DOCUMENT: usize = 512 * 1024;
const MAX_SCHEMA_FILE: usize = 512 * 1024;
const MAX_FILES: u32 = 128;
const MAX_PROJECTS: usize = 8;
const MAX_TABLES: usize = 256;
const MAX_COLUMNS: usize = 256;
const REFRESH: Duration = Duration::from_secs(2);
const DISCOVER: Duration = Duration::from_secs(30);
const SLICE: Duration = Duration::from_millis(15);
const KEYWORDS: &[&str] = &[
    "SELECT", "FROM", "WHERE", "AND", "OR", "NOT", "NULL", "IS", "IN", "EXISTS", "LIKE", "BETWEEN", "JOIN", "LEFT",
    "RIGHT", "INNER", "OUTER", "CROSS", "ON", "AS", "DISTINCT", "GROUP", "BY", "ORDER", "HAVING", "ASC", "DESC",
    "LIMIT", "OFFSET", "UNION", "ALL", "INSERT", "INTO", "VALUES", "UPDATE", "SET", "DELETE", "CASE", "WHEN", "THEN",
    "ELSE", "END", "COUNT", "SUM", "AVG", "MIN", "MAX",
];

pub(super) fn project_for(path: &str) -> Option<Project> {
    host::projects()
        .ok()?
        .into_iter()
        .filter(|p| {
            p.root().is_ok_and(|root| {
                path == root || path.strip_prefix(root.trim_end_matches('/')).is_some_and(|r| r.starts_with('/'))
            })
        })
        .max_by_key(|p| p.root().map_or(0, |r| r.len()))
}

#[derive(Clone, Debug, Default)]
pub(super) struct Table {
    name: String,
    columns: BTreeSet<String>,
}
type Schema = BTreeMap<String, Table>;

#[derive(Default)]
struct FileSchema {
    checked: Option<Instant>,
    tables: Schema,
}

struct ProjectSchema {
    discovered: Instant,
    used: Instant,
    files: BTreeMap<String, FileSchema>,
    merged: Schema,
}

#[derive(Default)]
pub(super) struct SchemaCache {
    projects: BTreeMap<String, ProjectSchema>,
    empty: Schema,
}

impl SchemaCache {
    // The SDK exposes synchronous reads, without stat or file events for providers.
    // Bound host calls and parsing; warm keystrokes do no filesystem work. Individual
    // host calls cannot be preempted, so the slice is a soft rather than hard deadline.
    pub(super) fn schema(&mut self, project: Option<&Project>) -> &Schema {
        let Some(project) = project else { return &self.empty };
        let Ok(root) = project.root() else { return &self.empty };
        let started = Instant::now();
        if !self.projects.contains_key(&root)
            && self.projects.len() >= MAX_PROJECTS
            && let Some(oldest) = self.projects.iter().min_by_key(|(_, p)| p.used).map(|(k, _)| k.clone())
        {
            self.projects.remove(&oldest);
        }
        let fresh = self.projects.get(&root).is_some_and(|p| p.discovered.elapsed() < DISCOVER);
        if !fresh {
            // Limit both discovery and retained metadata, including projects with many migrations.
            let mut paths = project.find_files("**/schema.sql", 16).unwrap_or_default();
            for path in project.find_files("**/*.sql", MAX_FILES).unwrap_or_default() {
                if paths.len() >= MAX_FILES as usize {
                    break;
                }
                if !paths.contains(&path) {
                    paths.push(path);
                }
            }
            let entry = self.projects.entry(root.clone()).or_insert_with(|| ProjectSchema {
                discovered: started,
                used: started,
                files: BTreeMap::new(),
                merged: Schema::new(),
            });
            let mut previous = std::mem::take(&mut entry.files);
            entry.files = paths
                .into_iter()
                .map(|p| {
                    let file = previous.remove(&p).unwrap_or_default();
                    (p, file)
                })
                .collect();
            entry.discovered = started;
        }
        let entry = self.projects.get_mut(&root).unwrap();
        entry.used = started;
        let mut dirty = !fresh;
        let mut pending: Vec<_> = entry
            .files
            .iter()
            .filter(|(_, f)| f.checked.is_none_or(|t| t.elapsed() >= REFRESH))
            .map(|(path, f)| (path.clone(), f.checked))
            .collect();
        pending.sort_by_key(|(p, checked)| (*checked, !p.ends_with("/schema.sql"), p.clone()));
        for (index, (path, _)) in pending.into_iter().take(8).enumerate() {
            if index > 0 && started.elapsed() >= SLICE {
                break;
            }
            let tables = project
                .read_file(&path)
                .ok()
                .filter(|text| text.len() <= MAX_SCHEMA_FILE)
                .map(|text| parse_schema(&text))
                .unwrap_or_default();
            entry.files.insert(path, FileSchema { checked: Some(Instant::now()), tables });
            dirty = true;
        }
        if dirty {
            entry.merged.clear();
            for file in entry.files.values() {
                for (key, table) in &file.tables {
                    if entry.merged.len() >= MAX_TABLES && !entry.merged.contains_key(key) {
                        continue;
                    }
                    let merged = entry
                        .merged
                        .entry(key.clone())
                        .or_insert_with(|| Table { name: table.name.clone(), columns: BTreeSet::new() });
                    for column in &table.columns {
                        if merged.columns.len() >= MAX_COLUMNS {
                            break;
                        }
                        merged.columns.insert(column.clone());
                    }
                }
            }
        }
        &entry.merged
    }
}

#[derive(Debug)]
struct Token {
    value: String,
    start: usize,
    end: usize,
    quoted: bool,
}
impl Token {
    fn is(&self, word: &str) -> bool {
        !self.quoted && self.value.eq_ignore_ascii_case(word)
    }
    fn name(&self) -> bool {
        self.quoted || self.value.chars().next().is_some_and(|c| c.is_alphabetic() || c == '_' || c == '$')
    }
}

fn word(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '$'
}

// Byte offsets are retained only internally; SDK text helpers construct UTF-16 ranges.
fn lex(text: &str) -> (Vec<Token>, Vec<(usize, usize)>) {
    let b = text.as_bytes();
    let mut i = 0;
    let mut tokens = Vec::new();
    let mut blocked = Vec::new();
    while i < b.len() {
        let start = i;
        if b[i].is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if text[i..].starts_with("--") || (b[i] == b'#' && !text[i..].starts_with("#{")) {
            i = text[i..].find('\n').map_or(b.len(), |n| i + n);
            blocked.push((start, i + 1));
        } else if text[i..].starts_with("/*") {
            let end = text[i + 2..].find("*/");
            i = end.map_or(b.len(), |n| i + n + 4);
            blocked.push((start, i + usize::from(end.is_none())));
        } else if text[i..].starts_with("#{") || text[i..].starts_with("${") {
            let end = text[i + 2..].find('}');
            i = end.map_or(b.len(), |n| i + n + 3);
            blocked.push((start, i + usize::from(end.is_none())));
        } else if matches!(b[i], b'\'' | b'"' | b'`' | b'[') {
            let close = if b[i] == b'[' { b']' } else { b[i] };
            let quoted = b[i] != b'\'';
            i += 1;
            let mut value = String::new();
            let mut closed = false;
            while i < b.len() {
                if b[i] == close {
                    i += 1;
                    if i < b.len() && b[i] == close {
                        value.push(close as char);
                        i += 1;
                        continue;
                    }
                    closed = true;
                    break;
                }
                if b[i] == b'\\' && i + 1 < b.len() {
                    i += 1;
                }
                let c = text[i..].chars().next().unwrap();
                value.push(c);
                i += c.len_utf8();
            }
            blocked.push((start, i + usize::from(!closed)));
            if quoted {
                tokens.push(Token { value, start, end: i, quoted });
            }
        } else {
            let c = text[i..].chars().next().unwrap();
            i += c.len_utf8();
            if word(c) {
                while i < b.len() {
                    let c = text[i..].chars().next().unwrap();
                    if !word(c) {
                        break;
                    }
                    i += c.len_utf8();
                }
            }
            tokens.push(Token { value: text[start..i].into(), start, end: i, quoted: false });
        }
    }
    (tokens, blocked)
}

fn qualified(tokens: &[Token], mut i: usize) -> Option<(String, usize)> {
    if !tokens.get(i)?.name() {
        return None;
    }
    let mut name = tokens[i].value.clone();
    i += 1;
    while tokens.get(i).is_some_and(|t| t.is(".")) && tokens.get(i + 1).is_some_and(Token::name) {
        name.push('.');
        name.push_str(&tokens[i + 1].value);
        i += 2;
    }
    Some((name, i))
}

fn parse_schema(text: &str) -> Schema {
    let (tokens, _) = lex(text);
    let mut out = Schema::new();
    let mut i = 0;
    while i < tokens.len() {
        if !tokens[i].is("CREATE") {
            i += 1;
            continue;
        }
        i += 1;
        while tokens
            .get(i)
            .is_some_and(|t| ["TEMPORARY", "TEMP", "GLOBAL", "LOCAL", "MEMORY", "CACHED"].iter().any(|w| t.is(w)))
        {
            i += 1;
        }
        if !tokens.get(i).is_some_and(|t| t.is("TABLE")) {
            continue;
        }
        i += 1;
        if tokens.get(i).is_some_and(|t| t.is("IF")) {
            if !tokens.get(i + 1).is_some_and(|t| t.is("NOT")) || !tokens.get(i + 2).is_some_and(|t| t.is("EXISTS")) {
                continue;
            }
            i += 3;
        }
        let Some((name, next)) = qualified(&tokens, i) else { continue };
        i = next;
        if !tokens.get(i).is_some_and(|t| t.is("(")) {
            continue;
        }
        i += 1;
        let mut depth = 1;
        let mut first = true;
        let mut columns = BTreeSet::new();
        while i < tokens.len() && depth > 0 {
            let t = &tokens[i];
            if depth == 1 && first {
                if t.name()
                    && (t.quoted
                        || ![
                            "PRIMARY",
                            "FOREIGN",
                            "UNIQUE",
                            "CONSTRAINT",
                            "CHECK",
                            "KEY",
                            "INDEX",
                            "FULLTEXT",
                            "SPATIAL",
                        ]
                        .iter()
                        .any(|w| t.is(w)))
                    && columns.len() < MAX_COLUMNS
                {
                    columns.insert(t.value.clone());
                }
                first = false;
            }
            if t.is("(") {
                depth += 1;
            }
            if t.is(")") {
                depth -= 1;
            }
            if depth == 1 && t.is(",") {
                first = true;
            }
            i += 1;
        }
        if out.len() < MAX_TABLES {
            out.insert(name.to_lowercase(), Table { name, columns });
        }
    }
    out
}

pub(super) struct Context {
    sql: String,
    cursor: usize,
    base: usize,
}

impl Context {
    pub(super) fn in_parameter(&self) -> bool {
        let before = &self.sql[..self.cursor];
        before.rfind("#{").is_some_and(|i| !before[i..].contains('}'))
    }

    pub(super) fn at(xml: &str, cursor: usize) -> Option<Self> {
        if xml.len() > MAX_DOCUMENT || cursor > xml.len() || !xml.is_char_boundary(cursor) {
            return None;
        }
        let mut stack: Vec<String> = Vec::new();
        let mut active: Option<(usize, Vec<u8>)> = None;
        let mut caret = false;
        let mut i = 0;
        while i < xml.len() {
            if xml[i..].starts_with("<!--") {
                i = xml[i + 4..].find("-->").map_or(xml.len(), |n| i + n + 7);
                continue;
            }
            if xml[i..].starts_with("<![CDATA[") {
                let start = i + 9;
                let end = xml[start..].find("]]>").map_or(xml.len(), |n| start + n);
                if let Some((base, masked)) = &mut active
                    && sql_stack(&stack)
                {
                    masked[start - *base..end - *base].copy_from_slice(&xml.as_bytes()[start..end]);
                    caret |= start <= cursor && cursor <= end;
                }
                i = (end + 3).min(xml.len());
                continue;
            }
            if xml.as_bytes()[i] == b'<' {
                let start = i;
                let mut quote = None;
                i += 1;
                while i < xml.len() {
                    let b = xml.as_bytes()[i];
                    if let Some(q) = quote {
                        if b == q {
                            quote = None;
                        }
                    } else if b == b'\'' || b == b'"' {
                        quote = Some(b);
                    } else if b == b'>' {
                        break;
                    }
                    i += 1;
                }
                if i == xml.len() {
                    break;
                }
                let tag = xml[start + 1..i].trim();
                i += 1;
                let closing = tag.starts_with('/');
                let name: String = tag
                    .trim_start_matches('/')
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                    .collect();
                if closing {
                    if let Some((base, masked)) = active.take() {
                        if super::STATEMENTS.contains(&name.as_str()) || name == "sql" {
                            if caret && cursor <= start {
                                return Some(Self {
                                    sql: String::from_utf8(masked).ok()?,
                                    cursor: cursor - base,
                                    base,
                                });
                            }
                        } else {
                            active = Some((base, masked));
                        }
                    }
                    if stack.last() == Some(&name) {
                        stack.pop();
                    }
                } else if !tag.starts_with(['!', '?']) && !tag.ends_with('/') {
                    if stack.last().is_some_and(|t| t == "mapper")
                        && (super::STATEMENTS.contains(&name.as_str()) || name == "sql")
                    {
                        active = Some((i, vec![b' '; xml.len() - i]));
                        caret = false;
                    }
                    stack.push(name);
                }
                continue;
            }
            let end = xml[i..].find('<').map_or(xml.len(), |n| i + n);
            if let Some((base, masked)) = &mut active
                && sql_stack(&stack)
            {
                masked[i - *base..end - *base].copy_from_slice(&xml.as_bytes()[i..end]);
                if i <= cursor && cursor <= end {
                    let before = &xml[i..cursor];
                    caret |= before.rfind('&').is_none_or(|a| before[a..].contains(';'));
                }
            }
            i = end;
        }
        let (base, masked) = active?;
        caret.then(|| Self { sql: String::from_utf8(masked).unwrap(), cursor: cursor - base, base })
    }
}

fn sql_stack(stack: &[String]) -> bool {
    stack
        .iter()
        .skip_while(|t| t.as_str() != "mapper")
        .skip(2)
        .all(|t| ["if", "choose", "when", "otherwise", "trim", "where", "set", "foreach"].contains(&t.as_str()))
}

fn table<'a>(schema: &'a Schema, name: &str) -> Option<&'a Table> {
    if let Some(t) = schema.get(&name.to_lowercase()) {
        return Some(t);
    }
    let mut matches =
        schema.values().filter(|t| t.name.rsplit('.').next().is_some_and(|n| n.eq_ignore_ascii_case(name)));
    let found = matches.next()?;
    matches.next().is_none().then_some(found)
}

fn reserved(t: &Token) -> bool {
    !t.name() || (!t.quoted && KEYWORDS.iter().any(|k| t.is(k)))
}

// Return table bindings from this query and its enclosing queries. Parentheses
// for function calls do not introduce scopes; subqueries do, and siblings don't leak.
fn bindings(tokens: &[Token], cursor: usize) -> BTreeMap<String, String> {
    let mut parents = vec![None];
    let mut scopes = Vec::new();
    let mut scope = 0;
    let mut parens = Vec::new();
    let mut levels = Vec::new();
    let mut bases = vec![0];
    let mut cursor_scope = 0;
    for (i, t) in tokens.iter().enumerate() {
        if t.is("UNION") {
            parents.push(parents[scope]);
            scope = parents.len() - 1;
            bases.push(parens.len());
        }
        if t.is("(") {
            parens.push(scope);
            if tokens.get(i + 1).is_some_and(|t| t.is("SELECT") || t.is("WITH")) {
                parents.push(Some(scope));
                scope = parents.len() - 1;
                bases.push(parens.len());
            }
        }
        scopes.push(scope);
        levels.push(parens.len());
        if t.start <= cursor {
            cursor_scope = scope;
        }
        if t.is(")") {
            scope = parens.pop().unwrap_or(0);
            if t.end <= cursor {
                cursor_scope = scope;
            }
        }
    }
    let mut chain = Vec::new();
    let mut next = Some(cursor_scope);
    while let Some(s) = next {
        chain.push(s);
        next = parents[s];
    }
    let mut out = BTreeMap::new();
    for s in chain.into_iter().rev() {
        let mut in_from = false;
        for (i, t) in tokens.iter().enumerate() {
            if scopes[i] != s || levels[i] != bases[s] {
                continue;
            }
            if ["WHERE", "GROUP", "ORDER", "HAVING", "SET", "VALUES", "UNION"].iter().any(|k| t.is(k)) {
                in_from = false;
            }
            let source = t.is("FROM") || t.is("JOIN") || t.is("UPDATE") || t.is("INTO") || (in_from && t.is(","));
            if t.is("FROM") {
                in_from = true;
            }
            if !source {
                continue;
            }
            if let Some((name, mut j)) = qualified(tokens, i + 1) {
                out.insert(name.to_lowercase(), name.clone());
                out.insert(name.rsplit('.').next().unwrap().to_lowercase(), name.clone());
                if tokens.get(j).is_some_and(|t| t.is("AS")) {
                    j += 1;
                }
                if let Some(alias) = tokens.get(j).filter(|t| !reserved(t)) {
                    out.insert(alias.value.to_lowercase(), name);
                }
            }
        }
    }
    out
}

fn insertion(name: &str) -> String {
    name.split('.').map(identifier_insertion).collect::<Vec<_>>().join(".")
}

fn identifier_insertion(part: &str) -> String {
    if part.chars().all(word)
        && part.chars().next().is_some_and(|c| !c.is_numeric())
        && !KEYWORDS.iter().any(|k| part.eq_ignore_ascii_case(k))
    {
        part.to_owned()
    } else {
        format!("`{}`", part.replace('`', "``"))
    }
}

pub(super) fn complete(context: Context, xml: &str, schema: &Schema) -> Vec<CompletionItem> {
    let (all, blocked) = lex(&context.sql);
    let cursor = context.cursor;
    if blocked.iter().any(|&(s, e)| s < cursor && cursor < e) {
        return Vec::new();
    }
    let start =
        context.sql[..cursor].char_indices().rev().take_while(|(_, c)| word(*c)).last().map_or(cursor, |(i, _)| i);
    let end = cursor + context.sql[cursor..].chars().take_while(|c| word(*c)).map(char::len_utf8).sum::<usize>();
    let prefix = &context.sql[start..cursor];
    // Semicolon-separated SQL statements cannot borrow each other's aliases.
    let left = all.iter().rfind(|t| t.start < cursor && t.is(";")).map_or(0, |t| t.end);
    let right = all.iter().find(|t| t.start >= cursor && t.is(";")).map_or(context.sql.len(), |t| t.start);
    let tokens: Vec<_> = all.into_iter().filter(|t| left <= t.start && t.start < right).collect();
    let before: Vec<_> = tokens.iter().filter(|t| t.end <= start).collect();
    let qualifier = before
        .last()
        .filter(|t| t.is("."))
        .and_then(|_| before.get(before.len().checked_sub(2)?))
        .filter(|t| t.name())
        .map(|t| t.value.as_str());
    let previous = before.last().copied();
    let comma_from = previous.is_some_and(|t| t.is(","))
        && before
            .iter()
            .rev()
            .find(|t| {
                ["FROM", "WHERE", "ON", "SELECT", "SET", "VALUES", "GROUP", "ORDER", "HAVING"].iter().any(|k| t.is(k))
            })
            .is_some_and(|t| t.is("FROM"));
    let table_position = previous.is_some_and(|t| ["FROM", "JOIN", "UPDATE", "INTO"].iter().any(|k| t.is(k)))
        || comma_from
        || (qualifier.is_some()
            && before
                .get(before.len().saturating_sub(3))
                .is_some_and(|t| ["FROM", "JOIN", "UPDATE", "INTO"].iter().any(|k| t.is(k))));
    let bindings = bindings(&tokens, cursor);
    let replace = Some(super::range_of(xml, context.base + start, context.base + end));
    let mut items = BTreeMap::new();
    let mut add = |label: &str, insert: String, kind: CompletionKind, detail: String| {
        if label.to_lowercase().starts_with(&prefix.to_lowercase()) {
            items.entry(label.to_owned()).or_insert_with(|| CompletionItem {
                replace,
                detail: Some(detail),
                insert_text: insert,
                ..CompletionItem::new(label, kind)
            });
        }
    };
    if table_position {
        for t in schema.values() {
            let label = if let Some(q) = qualifier {
                let Some((ns, leaf)) = t.name.rsplit_once('.') else { continue };
                if !ns.eq_ignore_ascii_case(q) {
                    continue;
                }
                leaf
            } else {
                &t.name
            };
            add(label, insertion(label), CompletionKind::Class, "SQL table · project schema".into());
        }
    } else if let Some(q) = qualifier {
        if let Some(t) = bindings.get(&q.to_lowercase()).and_then(|n| table(schema, n)) {
            for column in &t.columns {
                add(column, identifier_insertion(column), CompletionKind::Field, format!("SQL column · {}", t.name));
            }
        }
    } else {
        for name in bindings.values().collect::<BTreeSet<_>>() {
            if let Some(t) = table(schema, name) {
                for column in &t.columns {
                    add(
                        column,
                        identifier_insertion(column),
                        CompletionKind::Field,
                        format!("SQL column · {}", t.name),
                    );
                }
            }
        }
        for keyword in KEYWORDS {
            let insert = if !prefix.is_empty() && prefix.chars().all(|c| !c.is_ascii_uppercase()) {
                keyword.to_lowercase()
            } else {
                (*keyword).into()
            };
            add(keyword, insert, CompletionKind::Keyword, "SQL keyword".into());
        }
    }
    items.into_values().take(100).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Navigator;
    use openidea_plugin::mock::MockHost;
    use openidea_plugin::providers::{Document, Providers};
    use openidea_plugin::text::{offset, position};

    const DDL: &str = "-- users\nCREATE TABLE IF NOT EXISTS demo_users (id BIGINT PRIMARY KEY, name VARCHAR(80) NOT NULL, amount DECIMAL(10,2), CONSTRAINT uq UNIQUE(name)); CREATE TABLE roles (role_id INT, title VARCHAR(30), FOREIGN KEY (role_id) REFERENCES demo_users(id));";

    fn marked(fragment: &str) -> (String, usize) {
        let cursor = fragment.find('|').unwrap();
        (fragment.replacen('|', "", 1), cursor)
    }
    fn run(fragment: &str) -> Vec<CompletionItem> {
        let (xml, cursor) = marked(&format!("<mapper><select id=\"x\">{fragment}</select></mapper>"));
        Context::at(&xml, cursor).map_or_else(Vec::new, |ctx| complete(ctx, &xml, &parse_schema(DDL)))
    }
    fn labels(items: &[CompletionItem]) -> Vec<&str> {
        items.iter().map(|i| i.label.as_str()).collect()
    }

    #[test]
    fn parses_h2_mysql_quotes_types_constraints_and_multiple_tables() {
        let schema = parse_schema(&format!(
            "{DDL} /* CREATE TABLE fake(x int); */ CREATE TEMPORARY TABLE `app`.`order` (`key` int, `display name` varchar(255) DEFAULT 'a,b', \"updated_at\" timestamp, [state] int, KEY idx (`key`), CHECK (state in (1,2)));"
        ));
        assert_eq!(schema.len(), 3);
        assert_eq!(
            schema["demo_users"].columns.iter().map(String::as_str).collect::<Vec<_>>(),
            ["amount", "id", "name"]
        );
        assert_eq!(
            schema["app.order"].columns.iter().map(String::as_str).collect::<Vec<_>>(),
            ["display name", "key", "state", "updated_at"]
        );
        assert_eq!(schema["roles"].columns.len(), 2);
    }

    #[test]
    fn core_prefixes_and_aliases_use_metadata() {
        assert_eq!(labels(&run("SELECT u.na| FROM demo_users u")), ["name"]);
        assert_eq!(labels(&run("SELECT * FROM demo_u|")), ["demo_users"]);
        assert_eq!(labels(&run("SEL|")), ["SELECT"]);
        assert_eq!(run("sel|")[0].insert_text, "select");
        assert_eq!(labels(&run("SELECT r.ti| FROM demo_users AS u JOIN roles r ON r.role_id=u.id")), ["title"]);
        assert_eq!(labels(&run("SELECT na| FROM demo_users")), ["name"]);
        assert_eq!(labels(&run("UPDATE demo_users SET na| = #{name}")), ["name"]);
        assert_eq!(labels(&run("INSERT INTO demo_u| (id) VALUES (1)")), ["demo_users"]);
        assert_eq!(labels(&run("SELECT * FROM roles r, demo_u|")), ["demo_users"]);
        assert_eq!(labels(&run("SELECT u.na| FROM roles r, demo_users u")), ["name"]);
        assert_eq!(labels(&run("SELECT COUNT(u.na|) FROM demo_users u")), ["name"]);
    }

    #[test]
    fn scopes_do_not_leak_between_queries_or_mapper_statements() {
        assert!(run("SELECT r.ti| FROM demo_users u WHERE EXISTS (SELECT 1 FROM roles r)").is_empty());
        assert_eq!(labels(&run("SELECT * FROM demo_users u WHERE EXISTS (SELECT u.na| FROM roles r)")), ["name"]);
        assert_eq!(labels(&run("SELECT * FROM demo_users u WHERE EXISTS (SELECT u.ti| FROM roles u)")), ["title"]);
        assert!(run("SELECT * FROM demo_users u; SELECT u.na|").is_empty());
        assert!(run("SELECT * FROM roles r UNION SELECT r.ti| FROM demo_users u").is_empty());
        assert_eq!(labels(&run("SELECT * FROM roles r UNION SELECT u.na| FROM demo_users u")), ["name"]);
        let (xml, cursor) =
            marked("<mapper><select>SELECT * FROM demo_users u</select><select>SELECT u.na|</select></mapper>");
        assert!(complete(Context::at(&xml, cursor).unwrap(), &xml, &parse_schema(DDL)).is_empty());
    }

    #[test]
    fn sql_text_cdata_and_dynamic_tags_work() {
        assert_eq!(labels(&run("<![CDATA[ SELECT u.na| FROM demo_users u WHERE id < 3 ]]>")), ["name"]);
        assert_eq!(
            labels(&run("SELECT * FROM demo_users u <where><if test=\"name != null\">u.na| = #{name}</if></where>")),
            ["name"]
        );
        assert_eq!(
            labels(&run("SELECT * FROM demo_users <where><choose><when test=\"x\">na|</when></choose></where>")),
            ["name"]
        );
        let (xml, cursor) = marked("<mapper><sql id=\"fragment\">SEL|</sql></mapper>");
        assert_eq!(labels(&complete(Context::at(&xml, cursor).unwrap(), &xml, &Schema::new())), ["SELECT"]);
    }

    #[test]
    fn excludes_xml_attributes_comments_strings_and_parameters() {
        for fragment in [
            "SELECT 'na|' FROM demo_users",
            "SELECT 'escaped \\' na|' FROM demo_users",
            "SELECT 'it''s na|' FROM demo_users",
            "SELECT \"na|\" FROM demo_users",
            "SELECT `na|` FROM demo_users",
            "SELECT * FROM demo_users -- na|\n",
            "SELECT /* na| */ * FROM demo_users",
            "SELECT # na|\nFROM demo_users",
            "SELECT * FROM demo_users WHERE id=#{na|}",
            "SELECT * FROM ${na|}",
            "SELECT <!-- na| --> * FROM demo_users",
            "SELECT <if test=\"na|\">name</if> FROM demo_users",
            "SELECT <includ|e refid=\"x\"/>",
            "SELECT <bind name=\"x\" value=\"na|\"/>",
            "SELECT &l|t; FROM demo_users",
        ] {
            assert!(run(fragment).is_empty(), "{fragment}");
        }
        for input in [
            "<mapper namespace=\"na|\"></mapper>",
            "<mapper><select id=\"na|\">SELECT</select></mapper>",
            "<mapper><resultMap>na|</resultMap></mapper>",
            "<root><select>SEL|</select></root>",
        ] {
            let (xml, cursor) = marked(input);
            assert!(Context::at(&xml, cursor).is_none(), "{input}");
        }
        assert_eq!(labels(&run("SELECT 'closed' , na| FROM demo_users")), ["name"]);
    }

    #[test]
    fn replacement_preserves_alias_punctuation_suffix_and_utf16() {
        let (xml, cursor) =
            marked("<mapper><select>SELECT '😀汉字', u.na|me, u.id FROM demo_users u</select></mapper>");
        let item = complete(Context::at(&xml, cursor).unwrap(), &xml, &parse_schema(DDL)).remove(0);
        let range = item.replace.unwrap();
        assert_eq!(range.start, position(&xml, cursor - 2));
        assert_eq!(range.end, position(&xml, cursor + 2));
        let accepted = format!(
            "{}{}{}",
            &xml[..offset(&xml, range.start).unwrap()],
            item.insert_text,
            &xml[offset(&xml, range.end).unwrap()..]
        );
        assert!(accepted.contains("u.name, u.id"), "{accepted}");
    }

    #[test]
    fn provider_works_without_java_or_namespace_and_isolates_project_roots() {
        let host = MockHost::install();
        host.add_project("/project-other");
        host.add_file("/project/schema.sql", DDL);
        host.add_file("/project-other/schema.sql", "CREATE TABLE private_table(secret int);");
        let (xml, cursor) = marked("<mapper><select>SELECT * FROM demo_u|</select></mapper>");
        let mut nav = Navigator::default();
        let mut doc =
            Document { path: "/project/mapper.xml".into(), language: "xml".into(), version: 1, text: xml.clone() };
        assert_eq!(labels(&nav.completion(&doc, position(&xml, cursor), None)), ["demo_users"]);
        doc.path = "/project-other/mapper.xml".into();
        assert!(nav.completion(&doc, position(&xml, cursor), None).is_empty());
        doc.path = "/project-unknown/mapper.xml".into();
        assert!(project_for(&doc.path).is_none());
    }

    #[test]
    fn cache_reuses_metadata_refreshes_changes_and_discovers_new_files() {
        let host = MockHost::install();
        host.add_file("schema.sql", DDL);
        let project = project_for("/project/mapper.xml").unwrap();
        let mut cache = SchemaCache::default();
        assert!(cache.schema(Some(&project)).contains_key("demo_users"));
        host.add_file("schema.sql", "CREATE TABLE changed(fresh int);");
        assert!(cache.schema(Some(&project)).contains_key("demo_users"), "warm cache avoids a read");
        for file in cache.projects.get_mut("/project").unwrap().files.values_mut() {
            file.checked = Some(Instant::now() - REFRESH);
        }
        assert!(cache.schema(Some(&project)).contains_key("changed"));
        assert!(!cache.schema(Some(&project)).contains_key("demo_users"));
        host.add_file("new.sql", "CREATE TABLE added(value int);");
        cache.projects.get_mut("/project").unwrap().discovered = Instant::now() - DISCOVER;
        assert!(cache.schema(Some(&project)).contains_key("added"));
        host.add_file("schema.sql", "");
        for file in cache.projects.get_mut("/project").unwrap().files.values_mut() {
            file.checked = None;
        }
        assert!(!cache.schema(Some(&project)).contains_key("changed"));
    }

    #[test]
    fn discovery_is_bounded_and_prioritizes_schema_among_many_migrations() {
        let host = MockHost::install();
        host.add_file("src/main/resources/schema.sql", DDL);
        for i in 0..160 {
            host.add_file(&format!("migrations/{i:03}.sql"), "CREATE TABLE migration_table(col int);");
        }
        let project = project_for("/project/mapper.xml").unwrap();
        let mut cache = SchemaCache::default();
        assert!(cache.schema(Some(&project)).contains_key("demo_users"));
        assert_eq!(cache.projects["/project"].files.len(), MAX_FILES as usize);
        assert!(cache.projects["/project"].files.values().filter(|f| f.checked.is_some()).count() <= 8);
    }

    #[test]
    fn quoted_schema_tables_and_columns_insert_valid_identifiers() {
        let schema = parse_schema("CREATE TABLE `app`.`order` (`display name` varchar(20));");
        let (xml, cursor) = marked("<mapper><select>SELECT * FROM app.or|</select></mapper>");
        let item = complete(Context::at(&xml, cursor).unwrap(), &xml, &schema).remove(0);
        assert_eq!((item.label.as_str(), item.insert_text.as_str()), ("order", "`order`"));
        let (xml, cursor) = marked("<mapper><select>SELECT o.dis| FROM app.`order` o</select></mapper>");
        let item = complete(Context::at(&xml, cursor).unwrap(), &xml, &schema).remove(0);
        assert_eq!(item.insert_text, "`display name`");
    }

    #[test]
    #[ignore = "set OPENIDEA_MYBATIS_DEMO_ROOT to a real Java 8 demo; reads files only"]
    fn real_java8_project_read_only_smoke() {
        let root = std::env::var("OPENIDEA_MYBATIS_DEMO_ROOT").expect("demo project root");
        let ddl = std::fs::read_to_string(format!("{root}/src/main/resources/schema.sql")).unwrap();
        let xml = std::fs::read_to_string(format!("{root}/src/main/resources/mapper/UserMapper.xml")).unwrap();
        let host = MockHost::install();
        host.set_project_root(Some(&root));
        host.add_file("src/main/resources/schema.sql", &ddl);
        let mut nav = Navigator::default();
        for (sql, marker, expected) in [
            ("SELECT u.na FROM demo_users u", "u.na", "name"),
            ("SELECT * FROM demo_u", "demo_u", "demo_users"),
            ("SEL", "SEL", "SELECT"),
        ] {
            let text = xml.replace("SELECT id, name FROM demo_users WHERE id = #{id}", sql);
            assert_ne!(text, xml, "existing real demo query changed");
            let cursor = text.find(marker).unwrap() + marker.len();
            let doc = Document {
                path: format!("{root}/src/main/resources/mapper/UserMapper.xml"),
                language: "xml".into(),
                version: 1,
                text: text.clone(),
            };
            let started = Instant::now();
            let items = nav.completion(&doc, position(&text, cursor), None);
            eprintln!("real demo {sql:?}: {:?}, {:?}", labels(&items), started.elapsed());
            assert!(labels(&items).contains(&expected));
            let item = items.iter().find(|i| i.label == expected).unwrap();
            let range = item.replace.unwrap();
            let accepted = format!(
                "{}{}{}",
                &text[..offset(&text, range.start).unwrap()],
                item.insert_text,
                &text[offset(&text, range.end).unwrap()..]
            );
            assert!(accepted.contains(match expected {
                "name" => "SELECT u.name FROM demo_users u",
                "demo_users" => "SELECT * FROM demo_users",
                _ => "SELECT",
            }));
        }
    }
}
