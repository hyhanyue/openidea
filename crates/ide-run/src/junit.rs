//! JUnit XML test reports (Maven Surefire / Failsafe `TEST-*.xml`, Gradle
//! `build/test-results/**`): one result per test case, including the passed
//! ones that console output does not list.

use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum TestCaseStatus {
    Passed,
    Failed,
    Skipped,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum MavenTestPlugin {
    Surefire,
    Failsafe,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TestCaseReport {
    /// Maven module path for a reactor run, relative to the project where possible.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[specta(optional)]
    pub module: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[specta(optional)]
    pub maven_plugin: Option<MavenTestPlugin>,
    /// Fully qualified class (`com.acme.OrderTest`, nested: `com.acme.OrderTest$Nested`).
    pub class_name: String,
    /// Method name without `()`.
    pub name: String,
    pub status: TestCaseStatus,
    pub duration_ms: Option<f64>,
    /// Failure / error / skip message.
    pub message: Option<String>,
    /// Failure stack trace and captured output.
    pub output: Option<String>,
}

fn decode(s: &str) -> String {
    if !s.contains('&') {
        return s.to_owned();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        let Some(end) = tail.find(';').filter(|e| *e <= 10) else {
            out.push('&');
            rest = &tail[1..];
            continue;
        };
        let entity = &tail[1..end];
        let ch = match entity {
            "lt" => Some('<'),
            "gt" => Some('>'),
            "amp" => Some('&'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            e if e.starts_with("#x") => u32::from_str_radix(&e[2..], 16).ok().and_then(char::from_u32),
            e if e.starts_with('#') => e[1..].parse().ok().and_then(char::from_u32),
            _ => None,
        };
        match ch {
            Some(c) => {
                out.push(c);
                rest = &tail[end + 1..];
            }
            None => {
                out.push('&');
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn attrs(tag: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = tag;
    while let Some(eq) = rest.find('=') {
        let name = rest[..eq].trim().rsplit(char::is_whitespace).next().unwrap_or_default().to_owned();
        let after = rest[eq + 1..].trim_start();
        let Some(quote) = after.chars().next().filter(|c| *c == '"' || *c == '\'') else { break };
        let body = &after[1..];
        let Some(close) = body.find(quote) else { break };
        out.push((name, decode(&body[..close])));
        rest = &body[close + 1..];
    }
    out
}

fn attr<'a>(attrs: &'a [(String, String)], name: &str) -> Option<&'a str> {
    attrs.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
}

fn push_text(buf: &mut Option<String>, text: &str) {
    if let Some(b) = buf {
        b.push_str(text);
    }
}

/// Test cases of one report file.
pub fn parse_report(xml: &str) -> Vec<TestCaseReport> {
    let mut out = Vec::new();
    let mut current: Option<TestCaseReport> = None;
    // Text of the open <failure>/<error>/<skipped>/<system-out>/<system-err> element.
    let mut text: Option<String> = None;
    let mut rest = xml;
    while let Some(lt) = rest.find('<') {
        push_text(&mut text, &decode(&rest[..lt]));
        rest = &rest[lt..];
        if let Some(body) = rest.strip_prefix("<![CDATA[") {
            let end = body.find("]]>").unwrap_or(body.len());
            push_text(&mut text, &body[..end]);
            rest = body.get(end + 3..).unwrap_or_default();
            continue;
        }
        if rest.starts_with("<!--") {
            rest = rest.find("-->").map(|e| &rest[e + 3..]).unwrap_or_default();
            continue;
        }
        let Some(gt) = rest.find('>') else { break };
        let tag = &rest[1..gt];
        rest = &rest[gt + 1..];
        if tag.starts_with('?') || tag.starts_with('!') {
            continue;
        }
        if let Some(name) = tag.strip_prefix('/') {
            match name.trim() {
                "testcase" => out.extend(current.take()),
                "failure" | "error" | "rerunFailure" | "rerunError" | "skipped" | "system-out" | "system-err" => {
                    if let (Some(t), Some(c)) = (text.take(), current.as_mut()) {
                        let t = t.trim_matches('\n');
                        if !t.trim().is_empty() {
                            let o = c.output.get_or_insert_with(String::new);
                            if !o.is_empty() {
                                o.push('\n');
                            }
                            o.push_str(t);
                        }
                    }
                }
                _ => {}
            }
            continue;
        }
        let self_closing = tag.ends_with('/');
        let tag = tag.trim_end_matches('/');
        let name = tag.split(char::is_whitespace).next().unwrap_or_default();
        let a = attrs(&tag[name.len()..]);
        match name {
            "testcase" => {
                out.extend(current.take());
                let class_name = attr(&a, "classname").unwrap_or_default().to_owned();
                let test = attr(&a, "name").unwrap_or_default();
                // JUnit 5 display names may carry the signature: `shouldX()`, `add(int, int)[1]`.
                let base = test.split('(').next().unwrap_or(test).trim().to_owned();
                let duration_ms =
                    attr(&a, "time").and_then(|t| t.replace(',', "").parse::<f64>().ok()).map(|s| s * 1000.0);
                let case = TestCaseReport {
                    module: None,
                    maven_plugin: None,
                    class_name,
                    name: if base.is_empty() { test.to_owned() } else { base },
                    status: TestCaseStatus::Passed,
                    duration_ms,
                    message: None,
                    output: None,
                };
                if self_closing {
                    out.push(case);
                } else {
                    current = Some(case);
                }
            }
            "failure" | "error" | "skipped" => {
                if let Some(c) = current.as_mut() {
                    c.status = if name == "skipped" { TestCaseStatus::Skipped } else { TestCaseStatus::Failed };
                    c.message = attr(&a, "message").map(str::to_owned).or_else(|| c.message.take());
                    if !self_closing {
                        text = Some(String::new());
                    }
                }
            }
            // Flaky reruns: the test eventually passed; output only.
            "rerunFailure" | "rerunError" | "system-out" | "system-err" if !self_closing && current.is_some() => {
                text = Some(String::new());
            }
            _ => {}
        }
    }
    out.extend(current.take());
    out
}

/// A JVM test run's report scope, captured before spawning the process.
#[derive(Clone, Debug)]
pub enum ReportPlan {
    Maven { modules: Vec<MavenReportModule>, root: PathBuf, marker: Option<String>, reactor: bool },
    Gradle { cwd: PathBuf },
}

#[derive(Clone, Debug)]
pub struct MavenReportModule {
    pub dir: PathBuf,
    pub report_dirs: Vec<(PathBuf, MavenTestPlugin)>,
}

pub const MAVEN_RUN_PROPERTY: &str = "openidea.testRun";

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum MavenTestScope {
    Module(String),
    Reactor,
}

impl ReportPlan {
    pub(crate) fn maven_test_scope(&self) -> Option<MavenTestScope> {
        match self {
            Self::Maven { marker: Some(_), reactor: true, .. } => Some(MavenTestScope::Reactor),
            Self::Maven { modules, root, marker: Some(_), .. } => {
                modules.first().map(|module| MavenTestScope::Module(module_path(&module.dir, root)))
            }
            _ => None,
        }
    }

    pub fn reports(&self, started: SystemTime) -> Vec<TestCaseReport> {
        // Tagged reports can tolerate coarse mtimes without including an earlier Maven run.
        let coarse = started.checked_sub(Duration::from_secs(2)).unwrap_or(started);
        match self {
            Self::Maven { modules, root, marker, reactor } => {
                let since = if marker.is_some() { coarse } else { started };
                maven_reports(modules, root, marker.as_deref(), *reactor, since)
            }
            Self::Gradle { cwd } => {
                let dirs: Vec<_> = std::fs::read_dir(cwd.join("build/test-results"))
                    .into_iter()
                    .flatten()
                    .flatten()
                    .map(|entry| entry.path())
                    .filter(|p| p.is_dir())
                    .collect();
                report_files(&dirs, coarse)
                    .into_iter()
                    .filter_map(|file| std::fs::read_to_string(file).ok())
                    .flat_map(|xml| parse_report(&xml))
                    .collect()
            }
        }
    }
}

fn module_path(dir: &Path, root: &Path) -> String {
    let rel = dir.strip_prefix(root).unwrap_or(dir);
    if rel.as_os_str().is_empty() { ".".into() } else { ide_core::ipc_path::to_ipc(rel) }
}

#[derive(Default)]
struct ReportOwner {
    default_dir: bool,
    plugins: BTreeSet<MavenTestPlugin>,
}

fn maven_reports(
    modules: &[MavenReportModule],
    root: &Path,
    marker: Option<&str>,
    reactor: bool,
    since: SystemTime,
) -> Vec<TestCaseReport> {
    // One physical report directory can be configured in more than one module.
    let mut dirs = BTreeMap::<PathBuf, BTreeMap<usize, ReportOwner>>::new();
    for (index, module) in modules.iter().enumerate() {
        let target = module.dir.join("target");
        for (configured, plugin) in &module.report_dirs {
            if let Ok(dir) = dunce::canonicalize(configured) {
                dirs.entry(dir).or_default().entry(index).or_default().plugins.insert(*plugin);
            }
        }
        for (name, plugin) in
            [("surefire-reports", MavenTestPlugin::Surefire), ("failsafe-reports", MavenTestPlugin::Failsafe)]
        {
            if let Ok(dir) = dunce::canonicalize(target.join(name))
                && dir.starts_with(&target)
            {
                let owner = dirs.entry(dir).or_default().entry(index).or_default();
                owner.default_dir = true;
                owner.plugins.insert(plugin);
            }
        }
    }

    let mut reports = vec![Vec::new(); modules.len()];
    for (dir, owners) in dirs {
        for file in report_files(std::slice::from_ref(&dir), since) {
            // A file symlink could otherwise point at a report from another directory.
            if !std::fs::symlink_metadata(&file).is_ok_and(|m| m.is_file()) {
                continue;
            }
            let Ok(xml) = std::fs::read_to_string(file) else { continue };
            let properties = report_properties(&xml);
            if marker.is_some_and(|marker| properties.get(MAVEN_RUN_PROPERTY).map(String::as_str) != Some(marker)) {
                continue;
            }
            let owner = if let Some(basedir) = properties.get("basedir").filter(|_| marker.is_some()) {
                let Ok(basedir) = dunce::canonicalize(basedir) else { continue };
                owners.keys().copied().find(|&index| modules[index].dir == basedir)
            } else {
                // Legacy reports may omit basedir. Only an exclusive default
                // directory can supply module identity in that case.
                owners
                    .first_key_value()
                    .filter(|(_, owner)| owners.len() == 1 && owner.default_dir)
                    .map(|(&index, _)| index)
            };
            let Some(index) = owner else { continue };
            let module = reactor.then(|| module_path(&modules[index].dir, root));
            let plugins = &owners[&index].plugins;
            let plugin = match report_plugin(&xml) {
                Some(plugin) if marker.is_some() || plugins.contains(&plugin) => Some(plugin),
                Some(_) => None,
                None => plugins.first().copied().filter(|_| plugins.len() == 1),
            };
            reports[index].extend(parse_report(&xml).into_iter().map(|mut case| {
                case.module.clone_from(&module);
                case.maven_plugin = plugin;
                case
            }));
        }
    }
    reports.into_iter().flatten().collect()
}

fn report_plugin(xml: &str) -> Option<MavenTestPlugin> {
    let (_, suite) = xml.split_once("<testsuite ")?;
    let (tag, _) = suite.split_once('>')?;
    let attrs = attrs(tag);
    let schema = attr(&attrs, "xsi:noNamespaceSchemaLocation")?;
    match schema.rsplit('/').nth(2)? {
        "maven-surefire-plugin" => Some(MavenTestPlugin::Surefire),
        "maven-failsafe-plugin" => Some(MavenTestPlugin::Failsafe),
        _ => None,
    }
}

fn report_properties(xml: &str) -> BTreeMap<String, String> {
    let Some((_, properties)) = xml.split_once("<properties>") else { return BTreeMap::new() };
    let Some((properties, _)) = properties.split_once("</properties>") else { return BTreeMap::new() };
    properties
        .split("<property ")
        .skip(1)
        .filter_map(|property| {
            let (tag, _) = property.split_once('>')?;
            let attrs = attrs(tag);
            let name = attr(&attrs, "name")?;
            matches!(name, MAVEN_RUN_PROPERTY | "basedir").then_some(())?;
            Some((name.to_owned(), attr(&attrs, "value")?.to_owned()))
        })
        .collect()
}

/// Every `TEST-*.xml` in `dirs` written at or after `since`
/// (reports of earlier runs are ignored).
fn report_files(dirs: &[PathBuf], since: SystemTime) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = Vec::new();
    for dir in dirs {
        let Ok(rd) = std::fs::read_dir(dir) else { continue };
        for e in rd.flatten() {
            let path = e.path();
            let name = e.file_name().to_string_lossy().into_owned();
            if !(name.starts_with("TEST-") && name.ends_with(".xml")) {
                continue;
            }
            let fresh = e.metadata().and_then(|m| m.modified()).is_ok_and(|t| t >= since);
            if fresh {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}

#[cfg(test)]
mod tests {
    use super::*;

    const SUREFIRE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<testsuite xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" name="com.example.demo.todo.TodoServiceTests" time="0.05" tests="4" errors="0" skipped="1" failures="1">
  <properties>
    <property name="java.version" value="21"/>
  </properties>
  <testcase name="createsAndCompletesTodos" classname="com.example.demo.todo.TodoServiceTests" time="0.031"/>
  <testcase name="deleteReturnsFalseForUnknownId()" classname="com.example.demo.todo.TodoServiceTests" time="0.002">
    <system-out><![CDATA[hello <world>
]]></system-out>
  </testcase>
  <testcase name="fails" classname="com.example.demo.todo.TodoServiceTests" time="1,234.5">
    <failure message="expected: &lt;1&gt; but was: &lt;2&gt;" type="org.opentest4j.AssertionFailedError"><![CDATA[org.opentest4j.AssertionFailedError: expected: <1> but was: <2>
	at com.example.demo.todo.TodoServiceTests.fails(TodoServiceTests.java:30)
]]></failure>
  </testcase>
  <testcase name="later" classname="com.example.demo.todo.TodoServiceTests" time="0">
    <skipped message="not yet"/>
  </testcase>
</testsuite>
"#;

    #[test]
    fn parses_surefire_reports() {
        let cases = parse_report(SUREFIRE);
        assert_eq!(cases.len(), 4);
        assert_eq!(cases[0].name, "createsAndCompletesTodos");
        assert_eq!(cases[0].class_name, "com.example.demo.todo.TodoServiceTests");
        assert_eq!(cases[0].status, TestCaseStatus::Passed);
        assert_eq!(cases[0].duration_ms, Some(31.0));
        assert_eq!(cases[1].name, "deleteReturnsFalseForUnknownId");
        assert_eq!(cases[1].output.as_deref(), Some("hello <world>"));
        assert_eq!(cases[2].status, TestCaseStatus::Failed);
        assert_eq!(cases[2].message.as_deref(), Some("expected: <1> but was: <2>"));
        assert!(cases[2].output.as_deref().unwrap().contains("TodoServiceTests.java:30"));
        assert_eq!(cases[2].duration_ms, Some(1_234_500.0));
        assert_eq!(cases[3].status, TestCaseStatus::Skipped);
        assert_eq!(cases[3].message.as_deref(), Some("not yet"));
    }

    #[test]
    fn errors_reruns_and_entities() {
        let xml = r#"<testsuite><testcase classname="a.B$Inner" name="x(int)[1]" time="0.1"><error message="boom &amp; bust">trace &#65;&#x42;</error></testcase>
<testcase classname="a.B" name="flaky" time="0.2"><rerunFailure message="first">t1</rerunFailure></testcase>
<!-- <testcase classname="c.D" name="commented"/> --></testsuite>"#;
        let cases = parse_report(xml);
        assert_eq!(cases.len(), 2);
        assert_eq!(cases[0].class_name, "a.B$Inner");
        assert_eq!(cases[0].name, "x");
        assert_eq!(cases[0].status, TestCaseStatus::Failed);
        assert_eq!(cases[0].message.as_deref(), Some("boom & bust"));
        assert_eq!(cases[0].output.as_deref(), Some("trace AB"));
        assert_eq!(cases[1].status, TestCaseStatus::Passed, "passed on rerun");
        assert!(parse_report("not xml").is_empty());
    }

    #[test]
    fn identifies_plugins_after_resolving_the_report_owner() {
        use MavenTestPlugin::{Failsafe, Surefire};
        let tmp = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(tmp.path()).unwrap();
        let alpha = root.join("alpha");
        let beta = root.join("beta");
        let shared = root.join("reports");
        for dir in [&alpha, &beta, &shared] {
            std::fs::create_dir(dir).unwrap();
        }
        for (name, dir) in [("Alpha", &alpha), ("Beta", &beta)] {
            let xml = format!(
                r#"<testsuite><properties><property name="{MAVEN_RUN_PROPERTY}" value="current"/><property name="basedir" value="{}"/></properties><testcase classname="{name}" name="same"/></testsuite>"#,
                dir.display()
            );
            std::fs::write(shared.join(format!("TEST-{name}.xml")), xml).unwrap();
        }
        let mut plan = ReportPlan::Maven {
            modules: vec![
                MavenReportModule { dir: alpha, report_dirs: vec![(shared.clone(), Surefire)] },
                MavenReportModule { dir: beta, report_dirs: vec![(shared.clone(), Failsafe)] },
            ],
            root,
            marker: Some("current".into()),
            reactor: true,
        };
        let reports = plan.reports(SystemTime::UNIX_EPOCH);
        let sources: Vec<_> = reports.iter().map(|report| (report.module.as_deref(), report.maven_plugin)).collect();
        assert_eq!(sources, [(Some("alpha"), Some(Surefire)), (Some("beta"), Some(Failsafe))]);

        let ReportPlan::Maven { modules, .. } = &mut plan else { unreachable!() };
        modules[1].report_dirs.push((shared.clone(), Surefire));
        let reports = plan.reports(SystemTime::UNIX_EPOCH);
        assert_eq!(reports[1].module.as_deref(), Some("beta"));
        assert_eq!(reports[1].maven_plugin, None);

        let file = shared.join("TEST-Beta.xml");
        let xml = std::fs::read_to_string(&file).unwrap().replacen(
            "<testsuite>",
            r#"<testsuite xsi:noNamespaceSchemaLocation="https://maven.apache.org/surefire/maven-failsafe-plugin/xsd/failsafe-test-report.xsd">"#,
            1,
        );
        std::fs::write(file, xml).unwrap();
        assert_eq!(plan.reports(SystemTime::UNIX_EPOCH)[1].maven_plugin, Some(Failsafe));
    }

    #[test]
    fn reads_only_fresh_reports() {
        let tmp = tempfile::tempdir().unwrap();
        let module = tmp.path();
        let dir = module.join("target/surefire-reports");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::create_dir_all(module.join("build/test-results/test")).unwrap();
        std::fs::write(dir.join("TEST-a.B.xml"), SUREFIRE).unwrap();
        std::fs::write(dir.join("a.B.txt"), "ignored").unwrap();
        std::fs::write(module.join("build/test-results/test/TEST-c.D.xml"), r#"<testcase classname="c.D" name="g"/>"#)
            .unwrap();
        let module = dunce::canonicalize(module).unwrap();
        let maven = ReportPlan::Maven {
            modules: vec![MavenReportModule { dir: module.clone(), report_dirs: Vec::new() }],
            root: module.clone(),
            marker: None,
            reactor: false,
        };
        let gradle = ReportPlan::Gradle { cwd: module };
        let reports = maven.reports(SystemTime::UNIX_EPOCH);
        assert_eq!(reports.len(), 4);
        assert!(reports.iter().all(|r| r.maven_plugin == Some(MavenTestPlugin::Surefire)));
        assert_eq!(gradle.reports(SystemTime::UNIX_EPOCH).len(), 1);
        let future = SystemTime::now() + Duration::from_secs(3600);
        assert!(maven.reports(future).is_empty());
        assert!(gradle.reports(future).is_empty());
    }
}
