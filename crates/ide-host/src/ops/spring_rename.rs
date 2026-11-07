//! Orchestrate framework planning and JDT symbol renames before one preview.
use super::doc::blocking;
use ide_core::{DocId, IdeResult, Pos, error::ErrorKind};
use ide_lsp::types::Renamed;
use spring_refactor::{JavaRename, Plan};
use std::path::{Path, PathBuf};

fn declaration_renamed(edit: &ide_core::WorkspaceEdit, java: &JavaRename, input: &str) -> bool {
    let replacement = java.new_name.as_deref().unwrap_or(input);
    let canonical = std::fs::canonicalize(&java.path).unwrap_or_else(|_| PathBuf::from(&java.path));
    edit.files.iter().any(|f| match f {
        ide_core::FileEdit::Edit { path, edits, .. } if path.0 == Path::new(&java.path) || path.0 == canonical => {
            edits.iter().any(|e| {
                e.range.start == Pos::new(java.pos.line, java.pos.character)
                    && e.range.end
                        == Pos::new(java.pos.line, java.pos.character + java.old_name.encode_utf16().count() as u32)
                    && e.new_text == replacement
            })
        }
        _ => false,
    })
}

pub(super) async fn rename(
    project: std::sync::Arc<crate::Project>,
    doc: DocId,
    pos: Pos,
    new_name: String,
    cancel: ide_core::CancellationToken,
) -> IdeResult<Renamed> {
    let worker = project.clone();
    let token = cancel.clone();
    let input = new_name.clone();
    let (scope, plans, version) = blocking(move || {
        let info = worker.documents.snapshot(doc)?;
        let path = ide_core::ipc_path::to_ipc(&info.path);
        let scope = super::spring_refactor::Snapshot::load(&worker, doc, &token)?;
        let target = scope.as_ref().and_then(|s| s.target(&path, pos));
        if target.is_none() && scope.as_ref().is_some_and(|s| s.recognized(&path, pos)) {
            return Err(scope.as_ref().unwrap().refusal(&path, pos));
        }
        let plan = target.as_ref().map(|t| scope.as_ref().unwrap().plan(t, &input)).transpose()?;
        Ok((scope, plan.into_iter().collect::<Vec<_>>(), info.version))
    })
    .await?;
    let renamed = if plans.is_empty() {
        project.lsp.rename(doc, pos, new_name.clone(), cancel.clone()).await?
    } else {
        Renamed::default()
    };
    let input = new_name.clone();
    let verifier = project.clone();
    let (mut scope, mut renamed, plans) = blocking(move || {
        let mut found = plans;
        let mut renamed = renamed;
        if let Some(scope) = &scope {
            scope.refine_java_edits(&mut renamed.edit)?;
            // Resolve a rename started at a Java usage from JDT's declaration
            // edits, never from a matching word in a different Java scope.
            if found.is_empty() {
                for file in &renamed.edit.files {
                    if let ide_core::FileEdit::Edit { path, edits, .. } = file {
                        for edit in edits.iter().filter(|e| e.new_text == input) {
                            let path = ide_core::ipc_path::to_ipc(path);
                            let target = scope.target(&path, edit.range.start);
                            if target.is_none() && scope.recognized(&path, edit.range.start) {
                                return Err(scope.refusal(&path, edit.range.start));
                            }
                            if let Some(t) = target
                                && t.java.is_some()
                                && super::spring_refactor::range(t.range) == edit.range
                            {
                                found.push(scope.plan(&t, &input)?);
                            }
                        }
                    }
                }
            }
            scope.verify(&verifier)?;
        }
        Ok((scope, renamed, found))
    })
    .await?;
    // All requests describe the original documents. JDT may already include
    // accessor edits in the field response; identical declarations need no second request.
    for plan in &plans {
        for java in plan.java_requests() {
            if cancel.is_cancelled() {
                return Err(ide_core::IdeError::cancelled());
            }
            if declaration_renamed(&renamed.edit, java, &new_name) {
                continue;
            }
            let worker = project.clone();
            let path = PathBuf::from(&java.path);
            let info = blocking(move || worker.documents.open(&path)).await?;
            let result = project
                .lsp
                .rename(
                    info.id,
                    Pos::new(java.pos.line, java.pos.character),
                    java.new_name.clone().unwrap_or_else(|| new_name.clone()),
                    cancel.clone(),
                )
                .await?;
            let (returned_scope, combined) = blocking(move || {
                let mut result = result;
                if let Some(scope) = &scope {
                    scope.refine_java_edits(&mut result.edit)?;
                }
                renamed.edit.files.extend(result.edit.files);
                renamed.incomplete = renamed.incomplete.or(result.incomplete);
                renamed.edit.deduplicate();
                Ok((scope, renamed))
            })
            .await?;
            scope = returned_scope;
            renamed = combined;
            if !declaration_renamed(&renamed.edit, java, &new_name) {
                return Err(ide_core::IdeError::new(
                    ErrorKind::InvalidInput,
                    "Java language server did not rename the framework declaration",
                ));
            }
        }
    }
    if cancel.is_cancelled() {
        return Err(ide_core::IdeError::cancelled());
    }
    blocking(move || {
        if project.documents.version(doc).ok() != Some(version) {
            return Err(ide_core::IdeError::new(
                ErrorKind::OutOfSync,
                "document changed while computing rename; request it again",
            ));
        }
        if let Some(scope) = scope {
            scope.verify(&project)?;
            let framework_plan = !plans.is_empty();
            for plan in plans {
                scope.merge(&project, &mut renamed.edit, plan)?;
            }
            if !framework_plan && !renamed.edit.files.is_empty() {
                scope.merge(&project, &mut renamed.edit, Plan::default())?;
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
