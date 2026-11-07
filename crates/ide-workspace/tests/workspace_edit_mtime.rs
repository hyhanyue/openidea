#![cfg(target_os = "macos")]

use ide_core::{FileEdit, IdeEvent, LanguageId, Pos, Range, TextEdit, WorkspaceEdit};
use ide_workspace::Documents;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

#[test]
fn restored_java_source_invalidates_incremental_compiler_output() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("App.java");
    std::fs::write(&path, "old").unwrap();
    let old_time = SystemTime::UNIX_EPOCH + Duration::from_secs(10);
    std::fs::File::open(&path).unwrap().set_modified(old_time).unwrap();
    let applied = Arc::new(Mutex::new(None));
    let events = applied.clone();
    let docs = Documents::new(
        Arc::new(move |event| {
            if let IdeEvent::WorkspaceEditApplied { id, .. } = event {
                *events.lock().unwrap() = Some(id);
            }
        }),
        Box::new(|_, _| LanguageId("java".into())),
    );
    docs.apply_workspace_edit(&WorkspaceEdit {
        files: vec![FileEdit::Edit {
            path: path.clone().into(),
            version: None,
            edits: vec![TextEdit { range: Range::new(Pos::new(0, 0), Pos::new(0, 3)), new_text: "new".into() }],
        }],
        ..Default::default()
    })
    .unwrap();
    let id = applied.lock().unwrap().unwrap();
    docs.undo_workspace_edit(id).unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "old");
    assert!(std::fs::metadata(&path).unwrap().modified().unwrap() > old_time);
    docs.redo_workspace_edit(id).unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "new");
}
