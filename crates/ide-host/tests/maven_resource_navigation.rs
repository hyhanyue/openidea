use ide_core::{Location, NullSink, Pos, Range, error::ErrorKind};
use ide_host::{Host, HostConfig, ops};
use ide_lsp::types::GotoKind;
use std::{fs, sync::Arc};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn yaml_maven_placeholders_goto_pom_property_names() {
    let temp = tempfile::tempdir().unwrap();
    let base = dunce::canonicalize(temp.path()).unwrap();
    let root = base.join("project");
    let resources = root.join("src/main/resources");
    fs::create_dir_all(&resources).unwrap();
    let pom = root.join("pom.xml");
    fs::write(
        &pom,
        r#"<project xmlns="http://maven.apache.org/POM/4.0.0">
  <modelVersion>4.0.0</modelVersion>
  <groupId>example</groupId>
  <artifactId>demo</artifactId>
  <version>1.0.0</version>
  <properties>
    <project.port>8080</project.port>
    <!-- 👋 --><project.servlet.path>/demo</project.servlet.path>
  </properties>
  <build><resources><resource>
    <directory>src/main/resources</directory><filtering>true</filtering>
  </resource></resources></build>
</project>
"#,
    )
    .unwrap();

    let yaml = resources.join("application.yml");
    fs::write(&yaml, "server:\n  port: @project.port@\n  servlet:\n    context-path: @project.servlet.path@\n")
        .unwrap();

    let host = Host::new(HostConfig {
        config_dir: base.join("config"),
        data_dir: base.join("data"),
        local_data_dir: base.join("data"),
        languages: Arc::new(ide_language::LanguageRegistry::load(None).unwrap()),
        syntax: Arc::default(),
        sink_for: Box::new(|_| Arc::new(NullSink)),
        owner: |w| w,
        runtime: tokio::runtime::Handle::current(),
        project_environment: false,
    });
    let project = host.open_project("main", &root).unwrap();
    let doc = ops::doc::open(&host, "main", yaml).await.unwrap();
    assert_eq!(doc.language.as_str(), "yaml");

    // YAML's shared hover/click lookup sends the start of the placeholder name.
    let expected = [
        (Pos::new(1, 9), Range::new(Pos::new(6, 5), Pos::new(6, 17))),
        (Pos::new(3, 19), Range::new(Pos::new(7, 16), Pos::new(7, 36))),
        (Pos::new(3, 28), Range::new(Pos::new(7, 16), Pos::new(7, 36))),
    ];
    let mut actual = Vec::new();
    for (source, _) in expected {
        let found = ops::lsp::lsp_goto(&host, "main", doc.id, source, GotoKind::Definition).await;
        actual.push((source, found.map_err(|err| err.to_string())));
    }
    assert_eq!(actual, expected.map(|(source, range)| (source, Ok(vec![Location::file(pom.clone(), range)]))));

    let yaml = resources.join("application-dev.yaml");
    fs::write(
        &yaml,
        "label: '😀@project.port@'\nplain: project.port\nunclosed: '@project.port'\nunknown: '@missing@'\nspring: '${project.port}'\n",
    )
    .unwrap();
    let doc = ops::doc::open(&host, "main", yaml).await.unwrap();

    // A navigation target uses the current POM buffer, including unsaved edits.
    let pom_doc = ops::doc::open(&host, "main", pom.clone()).await.unwrap();
    project.documents.replace_all(pom_doc.id, &format!("\n{}", pom_doc.text)).unwrap();
    let found = ops::lsp::lsp_goto(&host, "main", doc.id, Pos::new(0, 11), GotoKind::Definition).await.unwrap();
    assert_eq!(found, [Location::file(pom, Range::new(Pos::new(7, 5), Pos::new(7, 17)))]);

    for pos in [Pos::new(1, 7), Pos::new(2, 12), Pos::new(3, 11), Pos::new(4, 12)] {
        let error = ops::lsp::lsp_goto(&host, "main", doc.id, pos, GotoKind::Definition).await.unwrap_err();
        assert_eq!(error.kind, ErrorKind::Unavailable, "{pos:?}");
    }
}
