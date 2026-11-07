//! Compile and exercise the actual thin host adapter without building a native app.
#![allow(dead_code)]
extern crate self as ide_lsp;
use std::{path::PathBuf, sync::Arc};
pub struct Project {
    pub root: PathBuf,
    pub documents: Arc<ide_workspace::Documents>,
    pub lsp: FakeLsp,
}

pub mod types {
    #[derive(Default)]
    pub struct Renamed {
        pub edit: ide_core::WorkspaceEdit,
    }
}

#[derive(Default)]
pub struct FakeLsp(pub std::sync::Mutex<Option<ide_core::WorkspaceEdit>>);
impl FakeLsp {
    pub async fn rename(
        &self,
        _doc: ide_core::DocId,
        _pos: ide_core::Pos,
        _new_name: String,
        cancel: ide_core::CancellationToken,
    ) -> ide_core::IdeResult<types::Renamed> {
        if cancel.is_cancelled() {
            return Err(ide_core::IdeError::cancelled());
        }
        Ok(types::Renamed { edit: self.0.lock().unwrap().take().unwrap_or_default() })
    }
}

mod doc {
    pub async fn blocking<T>(f: impl FnOnce() -> ide_core::IdeResult<T>) -> ide_core::IdeResult<T> {
        f()
    }
}

#[path = "../../../../../crates/ide-host/src/ops/spring_refactor.rs"]
mod adapter;
mod spring_refactor {
    pub(super) use super::adapter::*;
}
#[path = "../../../../../crates/ide-host/src/ops/spring_rename.rs"]
mod spring_rename;

#[cfg(test)]
mod tests {
    use super::*;
    use ide_core::{
        CancellationToken, ChangeSet, IdeEvent, LanguageId, Pos, TextChange, WorkspaceEdit, error::ErrorKind,
    };
    use std::sync::Mutex;

    fn setup() -> (tempfile::TempDir, Project, Arc<Mutex<Vec<u32>>>) {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("src/main/java")).unwrap();
        std::fs::create_dir_all(tmp.path().join("src/main/resources")).unwrap();
        std::fs::write(tmp.path().join("pom.xml"), "<project/>").unwrap();
        std::fs::write(tmp.path().join("src/main/java/Config.java"), "import org.springframework.context.annotation.Bean; class Config { @Bean(\"port\") Object port() { return null; } }").unwrap();
        std::fs::write(tmp.path().join("src/main/java/Use.java"), "import org.springframework.beans.factory.annotation.Qualifier; class Use { /*😀*/ @Qualifier(\"port\") Object p; }").unwrap();
        let events = Arc::new(Mutex::new(Vec::new()));
        let ids = events.clone();
        let documents = Arc::new(ide_workspace::Documents::new(
            Arc::new(move |e| {
                if let IdeEvent::WorkspaceEditApplied { id, .. } = e {
                    ids.lock().unwrap().push(id);
                }
            }),
            Box::new(|_, _| LanguageId("java".into())),
        ));
        let project = Project { root: tmp.path().to_path_buf(), documents, lsp: FakeLsp::default() };
        (tmp, project, events)
    }

    fn preview(p: &Project) -> WorkspaceEdit {
        let path = p.root.join("src/main/java/Config.java");
        let doc = p.documents.open(&path).unwrap();
        let scope = adapter::Snapshot::load(p, doc.id, &CancellationToken::new()).unwrap().unwrap();
        let col = doc.text.find("\"port\"").unwrap() + 1;
        let target = scope.target(&ide_core::ipc_path::to_ipc(&doc.path), Pos::new(0, col as u32)).unwrap();
        assert!(scope.recognized(&ide_core::ipc_path::to_ipc(&doc.path), Pos::new(0, col as u32)));
        let plan = scope.plan(&target, "channel").unwrap();
        let mut edit = WorkspaceEdit::default();
        scope.merge(p, &mut edit, plan).unwrap();
        edit
    }

    #[test]
    fn unsaved_source_preview_apply_grouped_undo_redo() {
        let (_tmp, p, events) = setup();
        let path = p.root.join("src/main/java/Use.java");
        let doc = p.documents.open(&path).unwrap();
        p.documents
            .edit(
                doc.id,
                doc.version,
                ChangeSet { changes: vec![TextChange { from: 0, to: 0, insert: "// unsaved\n".into() }] },
            )
            .unwrap();
        let edit = preview(&p);
        assert!(events.lock().unwrap().is_empty(), "preview must not write");
        assert!(!std::fs::read_to_string(&path).unwrap().contains("unsaved"));
        p.documents.apply_workspace_edit(&edit).unwrap();
        assert!(p.documents.snapshot(doc.id).unwrap().text.contains("@Qualifier(\"channel\")"));
        let id = events.lock().unwrap()[0];
        p.documents.undo_workspace_edit(id).unwrap();
        assert!(p.documents.snapshot(doc.id).unwrap().text.starts_with("// unsaved\n"));
        assert!(p.documents.snapshot(doc.id).unwrap().text.contains("@Qualifier(\"port\")"));
        assert!(!std::fs::read_to_string(&path).unwrap().contains("unsaved"));
        p.documents.redo_workspace_edit(id).unwrap();
        assert!(p.documents.snapshot(doc.id).unwrap().text.contains("@Qualifier(\"channel\")"));
    }

    #[test]
    fn external_change_invalidates_entire_preview() {
        let (_tmp, p, events) = setup();
        let edit = preview(&p);
        let path = p.root.join("src/main/java/Use.java");
        std::fs::write(&path, "external change").unwrap();
        assert_eq!(p.documents.apply_workspace_edit(&edit).unwrap_err().kind, ErrorKind::OutOfSync);
        assert!(std::fs::read_to_string(p.root.join("src/main/java/Config.java")).unwrap().contains("@Bean(\"port\")"));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "external change");
        assert!(events.lock().unwrap().is_empty());
    }

    #[test]
    fn adding_a_previously_unedited_definition_invalidates_preview() {
        let (_tmp, p, _) = setup();
        let edit = preview(&p);
        std::fs::write(p.root.join("src/main/java/New.java"), "new definition").unwrap();
        assert_eq!(p.documents.apply_workspace_edit(&edit).unwrap_err().kind, ErrorKind::OutOfSync);
    }

    #[test]
    fn new_unsaved_bean_and_mapper_after_snapshot_are_rejected() {
        for declaration in [
            "import org.springframework.context.annotation.Bean; class New { @Bean(\"port\") Object duplicate() { return null; } }",
            "package a; interface New { int find(); }",
        ] {
            let (_tmp, p, events) = setup();
            let origin = p.documents.open(&p.root.join("src/main/java/Config.java")).unwrap();
            let scope = adapter::Snapshot::load(&p, origin.id, &CancellationToken::new()).unwrap().unwrap();
            let path = p.root.join("src/main/java/New.java");
            std::fs::write(&path, "").unwrap();
            let new = p.documents.open(&path).unwrap();
            std::fs::remove_file(&path).unwrap();
            p.documents.replace_all(new.id, declaration).unwrap();
            assert!(p.documents.is_dirty(new.id).unwrap());
            assert!(!path.exists());
            assert_eq!(scope.verify(&p).unwrap_err().kind, ErrorKind::OutOfSync);
            assert!(events.lock().unwrap().is_empty());
        }
    }

    #[test]
    fn closing_a_captured_document_reports_source_conflict() {
        let (_tmp, p, _) = setup();
        let origin = p.documents.open(&p.root.join("src/main/java/Config.java")).unwrap();
        let usage = p.documents.open(&p.root.join("src/main/java/Use.java")).unwrap();
        p.documents.replace_all(usage.id, "// unsaved definition").unwrap();
        let scope = adapter::Snapshot::load(&p, origin.id, &CancellationToken::new()).unwrap().unwrap();
        // Saving preserves the exact text fingerprint. Closing must still report
        // the captured document lifecycle change consistently as OutOfSync.
        p.documents.save(usage.id).unwrap();
        p.documents.close(usage.id);
        assert_eq!(scope.verify(&p).unwrap_err().kind, ErrorKind::OutOfSync);
    }

    #[test]
    fn closing_or_moving_a_dirty_source_invalidates_snapshot() {
        for move_outside in [false, true] {
            let (_tmp, p, _) = setup();
            let origin = p.documents.open(&p.root.join("src/main/java/Config.java")).unwrap();
            let path = p.root.join("src/main/java/Use.java");
            let usage = p.documents.open(&path).unwrap();
            p.documents.replace_all(usage.id, "// unsaved content").unwrap();
            let scope = adapter::Snapshot::load(&p, origin.id, &CancellationToken::new()).unwrap().unwrap();
            p.documents.close(usage.id);
            if move_outside {
                let outside = p.root.join("Outside.java");
                std::fs::write(&outside, "").unwrap();
                let moved = p.documents.open(&outside).unwrap();
                p.documents.replace_all(moved.id, "// unsaved content").unwrap();
            }
            assert_eq!(scope.verify(&p).unwrap_err().kind, ErrorKind::OutOfSync);
        }
    }

    #[test]
    fn opening_a_previously_unscanned_source_invalidates_snapshot() {
        let (_tmp, p, _) = setup();
        let source_root = p.root.join("src/main/java");
        std::fs::write(source_root.join(".ignore"), "Ignored.java\n").unwrap();
        let path = source_root.join("Ignored.java");
        std::fs::write(&path, "import org.springframework.context.annotation.Bean; class Ignored { @Bean(\"port\") Object duplicate() { return null; } }").unwrap();
        let origin = p.documents.open(&source_root.join("Config.java")).unwrap();
        let scope = adapter::Snapshot::load(&p, origin.id, &CancellationToken::new()).unwrap().unwrap();
        assert!(!scope.sources.contains_key(&ide_core::ipc_path::to_ipc(&path)));
        let opened = p.documents.open(&path).unwrap();
        assert!(!opened.dirty);
        assert_eq!(scope.verify(&p).unwrap_err().kind, ErrorKind::OutOfSync);
    }

    #[test]
    fn grouped_undo_refreshes_source_mtime_for_incremental_java_compilation() {
        let (_tmp, p, events) = setup();
        let path = p.root.join("src/main/java/Config.java");
        let original_time = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(10);
        std::fs::File::open(&path).unwrap().set_modified(original_time).unwrap();
        let edit = preview(&p);
        p.documents.apply_workspace_edit(&edit).unwrap();
        p.documents.undo_workspace_edit(events.lock().unwrap()[0]).unwrap();
        assert!(std::fs::metadata(path).unwrap().modified().unwrap() > original_time);
    }

    #[test]
    fn unsaved_edit_and_cancel_do_not_partially_write() {
        let (_tmp, p, events) = setup();
        let edit = preview(&p);
        let path = p.root.join("src/main/java/Use.java");
        let doc = p.documents.open(&path).unwrap();
        p.documents
            .edit(
                doc.id,
                doc.version,
                ChangeSet { changes: vec![TextChange { from: 0, to: 0, insert: "// edit\n".into() }] },
            )
            .unwrap();
        assert_eq!(p.documents.apply_workspace_edit(&edit).unwrap_err().kind, ErrorKind::OutOfSync);
        let token = CancellationToken::new();
        token.cancel();
        assert_eq!(adapter::Snapshot::load(&p, doc.id, &token).err().unwrap().kind, ErrorKind::Cancelled);
        assert!(events.lock().unwrap().is_empty());
        assert!(std::fs::read_to_string(p.root.join("src/main/java/Config.java")).unwrap().contains("@Bean(\"port\")"));
    }

    #[test]
    fn sibling_modules_and_generated_sources_are_excluded() {
        let (_tmp, p, _) = setup();
        std::fs::create_dir_all(p.root.join("sibling/src/main/java")).unwrap();
        std::fs::write(p.root.join("sibling/pom.xml"), "<project/>").unwrap();
        std::fs::write(p.root.join("sibling/src/main/java/Other.java"), "@Bean(\"port\") class Other {}").unwrap();
        std::fs::create_dir_all(p.root.join("target/generated-sources")).unwrap();
        std::fs::write(p.root.join("target/generated-sources/A.java"), "@Bean(\"port\") class A {}").unwrap();
        let edit = preview(&p);
        assert_eq!(edit.files.len(), 2);
        assert!(!format!("{edit:?}").contains("sibling"));
    }

    #[test]
    fn implicit_class_self_reference_edits_precede_java_file_move_and_undo_together() {
        let (_tmp, p, events) = setup();
        std::fs::remove_file(p.root.join("src/main/java/Config.java")).unwrap();
        let path = p.root.join("src/main/java/Port.java");
        let moved = p.root.join("src/main/java/HttpPort.java");
        let text = "import org.springframework.stereotype.Service; import org.springframework.beans.factory.annotation.Qualifier; @Service class Port { @Qualifier(\"port\") Object self; }";
        std::fs::write(&path, text).unwrap();
        let info = p.documents.open(&path).unwrap();
        let at = text.find("class Port").unwrap() + 6;
        let scope = adapter::Snapshot::load(&p, info.id, &CancellationToken::new()).unwrap().unwrap();
        let target = scope.target(&ide_core::ipc_path::to_ipc(&path), Pos::new(0, at as u32)).unwrap();
        let plan = scope.plan(&target, "HttpPort").unwrap();
        let mut edit = WorkspaceEdit {
            files: vec![
                ide_core::FileEdit::Edit {
                    path: path.clone().into(),
                    version: None,
                    edits: vec![ide_core::TextEdit {
                        range: ide_core::Range::new(Pos::new(0, at as u32), Pos::new(0, at as u32 + 4)),
                        new_text: "HttpPort".into(),
                    }],
                },
                ide_core::FileEdit::Rename { from: path.clone().into(), to: moved.clone().into() },
            ],
            ..Default::default()
        };
        scope.merge(&p, &mut edit, plan).unwrap();
        p.documents.apply_workspace_edit(&edit).unwrap();
        let changed = std::fs::read_to_string(&moved).unwrap();
        assert!(changed.contains("class HttpPort"));
        assert!(changed.contains("@Qualifier(\"httpPort\")"));
        p.documents.undo_workspace_edit(events.lock().unwrap()[0]).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
        assert!(!moved.exists());
    }

    #[tokio::test]
    async fn orchestrator_computes_explicit_bean_edit_without_java_server() {
        let (_tmp, p, events) = setup();
        let info = p.documents.open(&p.root.join("src/main/java/Config.java")).unwrap();
        let at = info.text.find("\"port\"").unwrap() + 1;
        let result = spring_rename::rename(
            Arc::new(p),
            info.id,
            Pos::new(0, at as u32),
            "channel".into(),
            CancellationToken::new(),
        )
        .await
        .unwrap();
        assert_eq!(result.edit.files.len(), 2);
        assert!(events.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn missing_java_declaration_response_refuses_mapper_edit() {
        let (_tmp, p, events) = setup();
        std::fs::write(p.root.join("src/main/java/M.java"), "package a; interface M { int find(); }").unwrap();
        let path = p.root.join("src/main/resources/M.xml");
        std::fs::write(&path, "<mapper namespace='a.M'><select id='find'>select 1</select></mapper>").unwrap();
        let info = p.documents.open(&path).unwrap();
        let at = info.text.find("find").unwrap();
        let error = spring_rename::rename(
            Arc::new(p),
            info.id,
            Pos::new(0, at as u32),
            "lookup".into(),
            CancellationToken::new(),
        )
        .await
        .err()
        .unwrap();
        assert!(error.message.contains("did not rename"));
        assert!(events.lock().unwrap().is_empty());
        assert!(std::fs::read_to_string(path).unwrap().contains("id='find'"));
    }

    #[test]
    #[ignore = "requires JDK8/Maven and real JDT edits captured by scripts/spring-refactor-jdt.mjs"]
    fn java8_boot_mybatis_apply_and_grouped_undo() {
        let root = PathBuf::from(std::env::var("OPENIDEA_FIXTURE").expect("copy fixture first"));
        let events = Arc::new(Mutex::new(Vec::new()));
        let ids = events.clone();
        let documents = Arc::new(ide_workspace::Documents::new(
            Arc::new(move |e| {
                if let IdeEvent::WorkspaceEditApplied { id, .. } = e {
                    ids.lock().unwrap().push(id);
                }
            }),
            Box::new(|_, _| LanguageId("java".into())),
        ));
        let project = Project { root: root.clone(), documents, lsp: FakeLsp::default() };
        let maven = std::env::var("OPENIDEA_MAVEN").expect("Maven executable");
        let verify = |label: &str| {
            let mut command = std::process::Command::new(&maven);
            command.args(["-o", "-q"]);
            if label == "baseline" {
                command.arg("clean");
            }
            let result = command.arg("test").current_dir(&root).output().unwrap();
            std::fs::write(root.join(format!("target/{label}.log")), [&result.stdout[..], &result.stderr[..]].concat())
                .unwrap();
            assert!(result.status.success(), "Maven {label} failed: {}", String::from_utf8_lossy(&result.stdout));
        };
        verify("baseline");
        for (label, relative, needle, replacement) in [
            ("bean", "src/main/java/dev/openidea/refactor/Config.java", "\"port\"", "channel"),
            ("implicit-bean", "src/main/java/dev/openidea/refactor/Consumer.java", "\"counter\"", "total"),
            ("config-key", "src/main/resources/application.properties", "service.timeout=11", "service.delay"),
            ("config-prefix", "src/main/java/dev/openidea/refactor/Settings.java", "\"sample\"", "renamed"),
            ("mapper", "src/main/resources/mapper/UserMapper.xml", "id=\"find\" resultType", "lookup"),
        ] {
            let info = project.documents.open(&root.join(relative)).unwrap();
            let at = info.text.find(needle).unwrap()
                + match label {
                    "bean" | "implicit-bean" | "config-prefix" => 1,
                    "mapper" => 4,
                    _ => 0,
                };
            let sdk_pos = openidea_plugin::text::position(&info.text, at);
            let pos = Pos::new(sdk_pos.line, sdk_pos.character);
            let scope = adapter::Snapshot::load(&project, info.id, &CancellationToken::new()).unwrap().unwrap();
            let target = scope.target(&ide_core::ipc_path::to_ipc(&info.path), pos).unwrap();
            let plan = scope.plan(&target, replacement).unwrap();
            let mut edit = if plan.java.is_some() {
                serde_json::from_slice::<WorkspaceEdit>(
                    &std::fs::read(std::env::var("OPENIDEA_JAVA_EDITS").unwrap()).unwrap(),
                )
                .unwrap()
            } else {
                WorkspaceEdit::default()
            };
            scope.merge(&project, &mut edit, plan).unwrap();
            let before = std::fs::read(root.join(relative)).unwrap();
            project.documents.apply_workspace_edit(&edit).unwrap();
            verify(&format!("{label}-after"));
            let id = *events.lock().unwrap().last().unwrap();
            project.documents.undo_workspace_edit(id).unwrap();
            assert_eq!(std::fs::read(root.join(relative)).unwrap(), before);
            verify(&format!("{label}-undo"));
            println!("Java8 Boot start/binding/MyBatis query verified: {label} apply + grouped undo");
        }
    }
}
