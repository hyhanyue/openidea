//! Orchestrate framework planning and JDT symbol rename before one preview.
use super::doc::blocking;
use ide_core::{DocId, IdeResult, Pos, error::ErrorKind};
use ide_lsp::types::Renamed;
use std::path::{Path, PathBuf};

pub(super) async fn rename(
    project: std::sync::Arc<crate::Project>,
    doc: DocId,
    pos: Pos,
    new_name: String,
    cancel: ide_core::CancellationToken,
) -> IdeResult<Renamed> {
    let documents = project.documents.clone();
    let version = blocking(move || documents.version(doc)).await?;
    let worker = project.clone();
    let token = cancel.clone();
    let scope = blocking(move || super::spring_refactor::Snapshot::load(&worker, doc, &token)).await?;
    let path = ide_core::ipc_path::to_ipc(&project.documents.snapshot(doc)?.path);
    let target = scope.as_ref().and_then(|s| s.target(&path, pos));
    if target.is_none() && scope.as_ref().is_some_and(|s| s.recognized(&path, pos)) {
        return Err(scope.as_ref().unwrap().refusal(&path, pos));
    }
    let plan = target.as_ref().map(|t| scope.as_ref().unwrap().plan(t, &new_name)).transpose()?;
    let mut renamed = if let Some(plan) = &plan {
        if let Some(java) = &plan.java {
            let worker = project.clone();
            let java_path = PathBuf::from(&java.path);
            let info = blocking(move || worker.documents.open(&java_path)).await?;
            project
                .lsp
                .rename(info.id, Pos::new(java.pos.line, java.pos.character), new_name.clone(), cancel.clone())
                .await?
        } else {
            Renamed::default()
        }
    } else {
        project.lsp.rename(doc, pos, new_name.clone(), cancel.clone()).await?
    };
    if cancel.is_cancelled() {
        return Err(ide_core::IdeError::cancelled());
    }
    blocking(move || {
        if project.documents.version(doc)? != version {
            return Err(ide_core::IdeError::new(
                ide_core::error::ErrorKind::OutOfSync,
                "document changed while computing rename; request it again",
            ));
        }
        if let Some(scope) = scope {
            let mut plans = Vec::new();
            if let Some(plan) = plan {
                plans.push(plan);
            } else {
                // A rename started at a Java usage: let JDT resolve the symbol,
                // then recognize its edited declaration, never guess from spelling.
                for file in &renamed.edit.files {
                    if let ide_core::FileEdit::Edit { path, edits, .. } = file {
                        for edit in edits {
                            if edit.new_text != new_name {
                                continue;
                            }
                            let path = ide_core::ipc_path::to_ipc(path);
                            let target = scope.target(&path, edit.range.start);
                            if target.is_none() && scope.recognized(&path, edit.range.start) {
                                return Err(scope.refusal(&path, edit.range.start));
                            }
                            if let Some(t) = target
                                && t.java.is_some()
                                && super::spring_refactor::range(t.range) == edit.range
                            {
                                plans.push(scope.plan(&t, &new_name)?);
                            }
                        }
                    }
                }
            }
            let framework_plan = !plans.is_empty();
            for plan in plans {
                if let Some(java) = &plan.java {
                    let found = renamed.edit.files.iter().any(|f| match f {
                        ide_core::FileEdit::Edit { path, edits, .. } if path.0 == Path::new(&java.path) => {
                            edits.iter().any(|e| {
                                e.range.start == Pos::new(java.pos.line, java.pos.character) && e.new_text == new_name
                            })
                        }
                        _ => false,
                    });
                    if !found {
                        return Err(ide_core::IdeError::new(
                            ErrorKind::InvalidInput,
                            "Java language server did not rename the framework declaration",
                        ));
                    }
                }
                scope.merge(&project, &mut renamed.edit, plan)?;
            }
            // Source scanning cannot certify library/runtime/string references.
            // Keep Java-only results reviewable under the same source-scope note.
            if !framework_plan && !renamed.edit.files.is_empty() {
                scope.merge(&project, &mut renamed.edit, Default::default())?;
            }
            scope.verify(&project)?;
        }
        if cancel.is_cancelled() {
            return Err(ide_core::IdeError::cancelled());
        }
        project.documents.prepare_workspace_edit(&mut renamed.edit)?;
        Ok(renamed)
    })
    .await
}
