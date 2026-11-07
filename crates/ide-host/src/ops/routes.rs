//! Spring Web mappings, source side: the project's Java and Kotlin files that could
//! declare a route, with their text. The UI owns the annotation parser and the index
//! it feeds, so this side stays a cheap scan.

use crate::Host;
use ide_core::IdeResult;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Files handed out at most, and bytes in total: a project's controllers fit well
/// inside these, and the UI indexes them once.
const MAX_FILES: usize = 1024;
const MAX_BYTES: usize = 48 << 20;
/// Bound reads even when generated sources are unexpectedly large.
const MAX_FILE_BYTES: u64 = 4 << 20;

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RouteSource {
    pub path: ide_core::IpcPath,
    pub text: String,
}

/// The files Spring mappings can live in: Java and Kotlin sources.
fn declares_mappings(path: &Path) -> bool {
    matches!(path.extension().and_then(|e| e.to_str()), Some("java" | "kt" | "kts"))
}

/// Candidate source files under `root`: `ignore` rules and `excludes`
/// decide what counts, and the UI ranks the matches itself.
pub(crate) fn sources_in(root: &Path, excludes: &[String]) -> Vec<RouteSource> {
    let found: Mutex<Vec<RouteSource>> = Mutex::new(Vec::new());
    let bytes = AtomicUsize::new(0);
    let count = AtomicUsize::new(0);
    ide_vfs::walk_files(root, excludes, |path| {
        if !declares_mappings(path)
            || bytes.load(Ordering::Relaxed) >= MAX_BYTES
            || count.load(Ordering::Relaxed) >= MAX_FILES
        {
            return;
        }
        if std::fs::metadata(path).map_or(true, |m| m.len() > MAX_FILE_BYTES) {
            return;
        }
        let Ok(text) = std::fs::read_to_string(path) else { return };
        // Consumers of composed annotations and inherited contracts need not name a mapping.
        if !text.contains('@') && !text.contains("extends") && !text.contains("implements") && !text.contains(':') {
            return;
        }
        let Ok(mut found) = found.lock() else { return };
        if found.len() >= MAX_FILES || bytes.load(Ordering::Relaxed) + text.len() > MAX_BYTES {
            return;
        }
        bytes.fetch_add(text.len(), Ordering::Relaxed);
        count.fetch_add(1, Ordering::Relaxed);
        found.push(RouteSource { path: path.to_path_buf().into(), text });
    });
    let mut files = found.into_inner().unwrap_or_default();
    files.sort_by(|a, b| a.path.cmp(&b.path));
    files
}

/// The folders to scan: the project folder itself first, then content roots outside it
/// (`content_roots` lists only the ones outside, so the project folder is never implied —
/// forgetting it scans nothing).
fn scan_roots(root: &Path, content_roots: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut roots = vec![root.to_path_buf()];
    roots.extend(content_roots);
    roots
}

/// Every mapping source of the project, for the request-path index.
pub fn spring_route_sources(host: &Host, window: &str) -> IdeResult<Vec<RouteSource>> {
    let project = host.project(window)?;
    let excludes = project.search_excludes();
    let mut out = Vec::new();
    let mut bytes = 0;
    for root in scan_roots(&project.root, project.content_roots()) {
        if out.len() >= MAX_FILES || bytes >= MAX_BYTES {
            break;
        }
        for source in sources_in(&root, &excludes) {
            if out.len() >= MAX_FILES || bytes + source.text.len() > MAX_BYTES {
                break;
            }
            bytes += source.text.len();
            out.push(source);
        }
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out.dedup_by(|a, b| a.path == b.path);
    match out.len() {
        0 => tracing::warn!("spring routes: no mapping source under {:?} (excludes: {})", project.root, excludes.len()),
        n => tracing::info!("spring routes: {n} mapping sources under {:?}", project.root),
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_project_folder_is_scanned_even_without_other_content_roots() {
        let root = Path::new("/p/demo");
        assert_eq!(scan_roots(root, Vec::new()), [PathBuf::from("/p/demo")]);
        assert_eq!(
            scan_roots(root, vec![PathBuf::from("/p/elsewhere")]),
            [PathBuf::from("/p/demo"), PathBuf::from("/p/elsewhere")]
        );
    }

    #[test]
    fn only_java_and_kotlin_sources_are_read() {
        for (name, expected) in
            [("A.java", true), ("B.kt", true), ("C.kts", true), ("D.txt", false), ("pom.xml", false), ("noext", false)]
        {
            assert_eq!(declares_mappings(Path::new(name)), expected, "{name}");
        }
    }

    #[test]
    fn mapping_contracts_and_annotation_consumers_are_retained() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("src")).unwrap();
        std::fs::write(tmp.path().join("src/Ctl.java"), "class Ctl { @GetMapping(\"/a\") void a() {} }").unwrap();
        std::fs::write(tmp.path().join("src/Plain.java"), "class Plain { void a() {} }").unwrap();
        std::fs::write(tmp.path().join("src/Custom.java"), "@Api class Custom { @Read void a() {} }").unwrap();
        std::fs::write(tmp.path().join("src/Intermediate.java"), "interface Intermediate extends Api {}").unwrap();
        std::fs::write(tmp.path().join("src/Notes.txt"), "@GetMapping(\"/a\")").unwrap();
        let files = sources_in(tmp.path(), &[]);
        assert_eq!(files.len(), 3, "{:?}", files.iter().map(|f| &f.path).collect::<Vec<_>>());
        assert!(files.iter().any(|f| f.path.ends_with("src/Ctl.java")));
        assert!(files.iter().any(|f| f.path.ends_with("src/Custom.java")));
        assert!(files.iter().any(|f| f.path.ends_with("src/Intermediate.java")));
    }

    #[test]
    fn excluded_folders_are_skipped() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("module/src")).unwrap();
        std::fs::create_dir_all(tmp.path().join("build")).unwrap();
        std::fs::write(tmp.path().join("module/src/A.java"), "@RequestMapping(\"/a\")").unwrap();
        std::fs::write(tmp.path().join("build/B.java"), "@RequestMapping(\"/b\")").unwrap();
        let files = sources_in(tmp.path(), &["build/**".to_owned()]);
        assert_eq!(files.len(), 1, "{:?}", files.iter().map(|f| &f.path).collect::<Vec<_>>());
        assert!(files[0].path.ends_with("module/src/A.java"), "{:?}", files[0].path);
    }
}
