//! Thin source-snapshot adapter. The planner never owns writes or undo history.
use crate::Project;
use ide_core::error::ErrorKind;
use ide_core::{CancellationToken, DocId, FileEdit, IdeError, IdeResult, Pos, Range, TextEdit, WorkspaceEdit};
use openidea_plugin::Position;
use spring_refactor::{Sources, Target};
use std::path::{Path, PathBuf};

pub(super) fn position(pos: Pos) -> Position {
    Position { line: pos.line, character: pos.col }
}

pub(super) fn range(r: openidea_plugin::Range) -> Range {
    Range::new(Pos::new(r.start.line, r.start.character), Pos::new(r.end.line, r.end.character))
}

fn stale() -> IdeError {
    IdeError::new(ErrorKind::OutOfSync, "Spring/MyBatis source scope changed; request rename again")
}

fn supported(path: &Path) -> bool {
    matches!(path.extension().and_then(|s| s.to_str()), Some("java" | "xml" | "yml" | "yaml" | "properties"))
}

pub(super) struct Snapshot {
    pub sources: Sources,
    guard: WorkspaceEdit,
    roots: Vec<PathBuf>,
    versions: Vec<(DocId, PathBuf, u32)>,
}

impl Snapshot {
    /// Limit identity resolution to the nearest Maven/Gradle module's conventional
    /// source roots; do not traverse siblings, linked roots, symlinks or build output.
    pub fn load(project: &Project, doc: DocId, cancel: &CancellationToken) -> IdeResult<Option<Self>> {
        if cancel.is_cancelled() {
            return Err(IdeError::cancelled());
        }
        let info = project.documents.snapshot(doc)?;
        let origin = info.path.to_path_buf();
        if !supported(&origin) || !origin.starts_with(&project.root) {
            return Ok(None);
        }
        let module = origin
            .ancestors()
            .skip(1)
            .take_while(|p| p.starts_with(&project.root))
            .find(|p| ["pom.xml", "build.gradle", "build.gradle.kts"].iter().any(|name| p.join(name).is_file()));
        let Some(module) = module else { return Ok(None) };
        let roots: Vec<_> = ["src/main/java", "src/main/resources", "src/test/java", "src/test/resources"]
            .iter()
            .map(|p| module.join(p))
            .filter(|p| p.is_dir() && !p.is_symlink())
            .collect();
        if !roots.iter().any(|p| origin.starts_with(p)) {
            return Ok(None);
        }
        let mut guard = WorkspaceEdit {
            files: roots
                .iter()
                .map(|p| FileEdit::Edit { path: p.clone().into(), edits: Vec::new(), version: None })
                .collect(),
            ..Default::default()
        };
        project.documents.prepare_workspace_edit(&mut guard)?;
        let mut paths = Vec::new();
        for root in &roots {
            for entry in ignore::WalkBuilder::new(root).hidden(false).follow_links(false).build() {
                if cancel.is_cancelled() {
                    return Err(IdeError::cancelled());
                }
                let entry = entry.map_err(|e| IdeError::internal(e.to_string()))?;
                if entry.file_type().is_some_and(|t| t.is_file()) && supported(entry.path()) {
                    if entry.metadata().map_err(|e| IdeError::internal(e.to_string()))?.len() > 2 * 1024 * 1024
                        || paths.len() >= 1500
                    {
                        return Err(IdeError::new(
                            ErrorKind::InvalidInput,
                            "Spring/MyBatis source scope exceeds the rename limit",
                        ));
                    }
                    paths.push(entry.into_path());
                }
            }
        }
        // Include unsaved new files, too. They may not exist in the filesystem walk.
        let mut versions = Vec::new();
        for id in project.documents.open_documents() {
            let snap = project.documents.snapshot(id)?;
            let path = snap.path.to_path_buf();
            if supported(&path) && roots.iter().any(|r| path.starts_with(r)) {
                paths.push(path.clone());
                versions.push((id, path, snap.version));
            }
        }
        paths.sort();
        paths.dedup();
        let mut sources = Sources::new();
        let mut bytes = 0;
        for path in paths {
            if cancel.is_cancelled() {
                return Err(IdeError::cancelled());
            }
            let text = project
                .documents
                .search_text_of(&path)
                .map(Ok)
                .unwrap_or_else(|| project.documents.load_file(&path).map(|l| l.text))?;
            bytes += text.len();
            if bytes > 32 * 1024 * 1024 {
                return Err(IdeError::new(
                    ErrorKind::InvalidInput,
                    "Spring/MyBatis source scope exceeds the rename limit",
                ));
            }
            sources.insert(ide_core::ipc_path::to_ipc(&path), text);
        }
        let out = Self { sources, guard, roots, versions };
        out.verify(project)?;
        Ok(Some(out))
    }

    pub fn target(&self, path: &str, pos: Pos) -> Option<Target> {
        spring_refactor::target(&self.sources, path, position(pos))
    }

    pub fn recognized(&self, path: &str, pos: Pos) -> bool {
        spring_refactor::recognized(&self.sources, path, position(pos))
    }

    pub fn refusal(&self, path: &str, pos: Pos) -> IdeError {
        IdeError::new(ErrorKind::InvalidInput, spring_refactor::refusal(&self.sources, path, position(pos)))
    }

    pub fn verify(&self, project: &Project) -> IdeResult<()> {
        let mut now = self.guard.clone();
        now.preconditions.clear();
        project.documents.prepare_workspace_edit(&mut now)?;
        if self.guard.preconditions != now.preconditions {
            return Err(stale());
        }
        for (doc, path, version) in &self.versions {
            let (current_path, current_version) = project.documents.identity(*doc).map_err(|_| stale())?;
            if current_path != *path || current_version != *version {
                return Err(stale());
            }
        }
        // Detect a document opened/edited while the scan/LSP was running.
        for id in project.documents.open_documents() {
            let snap = project.documents.snapshot(id).map_err(|_| stale())?;
            if supported(&snap.path) && self.roots.iter().any(|root| snap.path.starts_with(root)) {
                match self.sources.get(&ide_core::ipc_path::to_ipc(&snap.path)) {
                    Some(before) if &snap.text == before => {}
                    _ => return Err(stale()),
                }
            }
        }
        Ok(())
    }

    pub fn plan(&self, target: &Target, new_name: &str) -> IdeResult<spring_refactor::Plan> {
        spring_refactor::plan(&self.sources, target, new_name).map_err(|s| IdeError::new(ErrorKind::InvalidInput, s))
    }

    /// Merge only identical or disjoint text edits. Never silently resolve a
    /// disagreement between JDT and a framework identity by dropping one edit.
    pub fn merge(&self, project: &Project, edit: &mut WorkspaceEdit, plan: spring_refactor::Plan) -> IdeResult<()> {
        let mut additions = Vec::new();
        for (path, changes) in plan.files {
            let edits = changes.into_iter().map(|e| TextEdit { range: range(e.range), new_text: e.text }).collect();
            let version = project
                .documents
                .by_path(Path::new(&path))
                .map(|id| project.documents.version(id))
                .transpose()?
                .map(|v| v as i32);
            additions.push(FileEdit::Edit { path: PathBuf::from(path).into(), edits, version });
        }
        // Framework ranges describe the source snapshot before JDT's file move.
        // Keep them in that text-edit batch so offsets are applied together.
        let before_resources =
            edit.files.iter().position(|f| !matches!(f, FileEdit::Edit { .. })).unwrap_or(edit.files.len());
        if !additions.is_empty() && edit.files[before_resources..].iter().any(|f| matches!(f, FileEdit::Edit { .. })) {
            return Err(IdeError::new(
                ErrorKind::InvalidInput,
                "Framework rename cannot merge text edits after file operations",
            ));
        }
        edit.files.splice(before_resources..before_resources, additions);
        edit.deduplicate();
        for f in &edit.files {
            if let FileEdit::Edit { edits, .. } = f {
                for (i, a) in edits.iter().enumerate() {
                    for b in &edits[i + 1..] {
                        if a.range.start < b.range.end && b.range.start < a.range.end {
                            return Err(IdeError::new(
                                ErrorKind::InvalidInput,
                                "Java and framework rename edits overlap",
                            ));
                        }
                    }
                }
            }
        }
        self.verify(project)?;
        project.documents.prepare_workspace_edit(edit)?;
        // Includes the entire scanned scope, so newly added definitions and
        // previously unedited references also invalidate the preview.
        edit.preconditions.extend(self.guard.preconditions.clone());
        Ok(())
    }
}
