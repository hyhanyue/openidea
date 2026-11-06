use ide_core::{Location, NullSink, Pos, Range};
use ide_host::{Host, HostConfig, ops};
use ide_lsp::types::GotoKind;
use std::{fs, path::Path, sync::Arc};

const PARENT: &str = r#"<project>
  <modelVersion>4.0.0</modelVersion>
  <groupId>org.example</groupId><artifactId>parent</artifactId><version>1</version><packaging>pom</packaging>
  <properties>
    <!-- 😀 --><example.version>1.2.3</example.version>
  </properties>
  <dependencies><dependency><groupId>org.example</groupId><artifactId>library</artifactId><version>0</version></dependency></dependencies>
  <build><plugins><plugin><artifactId>example-plugin</artifactId><dependencies><dependency><groupId>org.example</groupId><artifactId>library</artifactId><version>0</version></dependency></dependencies></plugin></plugins></build>
  <dependencyManagement><dependencies>
    <dependency><groupId>other</groupId><artifactId>library</artifactId><version>2</version></dependency>
    <dependency><groupId>org.example</groupId><artifactId>library</artifactId><type>test-jar</type><version>3</version></dependency>
    <dependency><groupId>org.example</groupId><artifactId>library</artifactId><classifier>tests</classifier><version>4</version></dependency>
    <dependency><!-- 😀 --><groupId>org.example</groupId><artifactId>library</artifactId><version>${example.version}</version></dependency>
  </dependencies></dependencyManagement>
</project>
"#;

const CHILD: &str = r#"<project>
  <modelVersion>4.0.0</modelVersion>
  <parent><groupId>org.example</groupId><artifactId>parent</artifactId><version>1</version></parent>
  <artifactId>child</artifactId>
  <description>😀 ${example.version} ${unknown.version}</description>
  <dependencies>
    <dependency><groupId>org.example</groupId><artifactId>library</artifactId></dependency>
  </dependencies>
</project>
"#;

fn range(line: u32, start: u32, end: u32) -> Range {
    Range::new(Pos::new(line, start), Pos::new(line, end))
}

fn token(text: &str, line: u32, token: &str) -> Range {
    let row = text.lines().nth(line as usize).unwrap();
    let start = row[..row.find(token).unwrap()].encode_utf16().count() as u32;
    range(line, start, start + token.encode_utf16().count() as u32)
}

fn version_tag(text: &str, line: u32) -> Range {
    let tag = token(text, line, "<version>");
    range(line, tag.start.col + 1, tag.end.col - 1)
}

fn write(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pom_definitions_follow_the_effective_parent_and_open_buffers() {
    let temp = tempfile::tempdir().unwrap();
    let base = dunce::canonicalize(temp.path()).unwrap();
    let root = base.join("project");
    let pom = root.join("pom.xml");
    let child = root.join("child/pom.xml");
    let repository = base.join("repository");
    let cached = repository.join("org/example/parent/1/parent-1.pom");
    write(&pom, PARENT);
    write(&child, CHILD);
    write(&cached, PARENT);
    write(
        &base.join("config/settings.toml"),
        &format!("[languages.java.maven]\nlocalRepository = {:?}\n", repository.to_str().unwrap()),
    );

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
    let source = ops::doc::open(&host, "main", child.clone()).await.unwrap();
    let parent = ops::doc::open(&host, "main", pom.clone()).await.unwrap();
    let go = |pos| ops::lsp::lsp_goto(&host, "main", source.id, pos, GotoKind::Definition);
    let property = token(CHILD, 4, "example.version").start;
    let artifact = token(CHILD, 6, ">library<").start;
    let dependency = Pos::new(artifact.line, artifact.col + 1);

    let expected_property = Location::file(pom.clone(), token(PARENT, 4, "example.version"));
    let expected_version = Location::file(pom.clone(), version_tag(PARENT, 12));
    assert_eq!(go(property).await.unwrap(), [expected_property]);
    assert_eq!(go(dependency).await.unwrap(), [expected_version]);

    let edited_parent = format!("\n{PARENT}");
    project.documents.replace_all(parent.id, &edited_parent).unwrap();
    assert_eq!(go(property).await.unwrap(), [Location::file(pom.clone(), token(&edited_parent, 5, "example.version"))]);
    let override_pom = CHILD.replace(
        "  <artifactId>child</artifactId>",
        "  <artifactId>child</artifactId>\n  <properties><example.version>9.0.0</example.version></properties>",
    );
    project.documents.replace_all(source.id, &override_pom).unwrap();
    assert_eq!(
        go(token(&override_pom, 5, "example.version").start).await.unwrap(),
        [Location::file(child.clone(), token(&override_pom, 4, "example.version"))]
    );
    assert_eq!(
        go(Pos::new(dependency.line + 1, dependency.col)).await.unwrap(),
        [Location::file(pom.clone(), version_tag(&edited_parent, 13))]
    );
    assert!(go(token(&override_pom, 5, "unknown.version").start).await.unwrap().is_empty());

    let profile = PARENT.replace(
        "</project>",
        r#"  <profiles><profile><id>custom</id><activation><activeByDefault>true</activeByDefault></activation>
    <properties><example.version>5.0.0</example.version></properties>
  </profile></profiles>
</project>"#,
    );
    project.documents.replace_all(source.id, CHILD).unwrap();
    project.documents.replace_all(parent.id, &profile).unwrap();
    assert_eq!(go(property).await.unwrap(), [Location::file(pom.clone(), token(&profile, 15, "example.version"))]);
    project.documents.replace_all(parent.id, &edited_parent).unwrap();

    for edit in [
        CHILD.replace(">org.example</groupId><artifactId>library", ">unknown</groupId><artifactId>library"),
        CHILD.replace("</artifactId></dependency>", "</artifactId><type>war</type></dependency>"),
        CHILD.replace("</artifactId></dependency>", "</artifactId><classifier>missing</classifier></dependency>"),
    ] {
        project.documents.replace_all(source.id, &edit).unwrap();
        let declaration = token(&edit, dependency.line, ">library<").start;
        assert!(go(Pos::new(declaration.line, declaration.col + 1)).await.unwrap().is_empty());
    }

    for (coordinate, line) in [("<type>test-jar</type>", 10), ("<classifier>tests</classifier>", 11)] {
        let edited = CHILD.replace("</artifactId></dependency>", &format!("</artifactId>{coordinate}</dependency>"));
        project.documents.replace_all(source.id, &edited).unwrap();
        assert_eq!(go(dependency).await.unwrap(), [Location::file(pom.clone(), version_tag(&edited_parent, line + 1))]);
    }

    let local_management = |version: &str| {
        CHILD.replace(
        "</project>",
        &format!("  <dependencyManagement><dependencies>\n    <dependency><groupId>org.example</groupId><artifactId>library</artifactId>{version}</dependency>\n  </dependencies></dependencyManagement>\n</project>"),
    )
    };
    let managed_locally = local_management("<version>2.0.0</version>");
    project.documents.replace_all(source.id, &managed_locally).unwrap();
    assert_eq!(go(dependency).await.unwrap(), [Location::file(child.clone(), version_tag(&managed_locally, 9))]);
    project.documents.replace_all(source.id, &local_management("")).unwrap();
    assert!(go(dependency).await.unwrap().is_empty());

    project.documents.replace_all(source.id, CHILD).unwrap();
    let intermediate =
        CHILD.replace("<artifactId>child</artifactId>", "<artifactId>child</artifactId><packaging>pom</packaging>");
    project.documents.replace_all(source.id, &intermediate).unwrap();
    let grandchild = child.parent().unwrap().join("grandchild/pom.xml");
    let grandchild_pom = CHILD
        .replace("<artifactId>child</artifactId>", "<artifactId>grandchild</artifactId>")
        .replace("<artifactId>parent</artifactId>", "<artifactId>child</artifactId>");
    write(&grandchild, &grandchild_pom);
    let grandchild = ops::doc::open(&host, "main", grandchild).await.unwrap();
    for (pos, range) in
        [(property, token(&edited_parent, 5, "example.version")), (dependency, version_tag(&edited_parent, 13))]
    {
        let result = ops::lsp::lsp_goto(&host, "main", grandchild.id, pos, GotoKind::Definition).await.unwrap();
        assert_eq!(result, [Location::file(pom.clone(), range)]);
    }

    // Same group and artifact, but the relative parent's version no longer matches.
    project.documents.replace_all(parent.id, &PARENT.replace("<version>1</version>", "<version>9</version>")).unwrap();
    project.documents.replace_all(source.id, CHILD).unwrap();
    for (pos, target_range) in [(property, token(PARENT, 4, "example.version")), (dependency, version_tag(PARENT, 12))]
    {
        let found = go(pos).await.unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].range, target_range);
        let uri = found[0].uri.as_ref().unwrap();
        let virtual_doc = ops::lsp::lsp_virtual_document(&host, "main", uri.clone()).await.unwrap();
        assert_eq!(virtual_doc.description.as_deref(), cached.to_str());
        let editor = ops::doc::doc_open_virtual(&host, "main", uri.clone()).await.unwrap();
        assert!(editor.read_only);
        assert_eq!(editor.text, PARENT);
    }

    // An explicitly empty relative path also selects the repository parent,
    // even when the workspace has a POM with the requested coordinates.
    project.documents.replace_all(parent.id, &edited_parent).unwrap();
    let repository_only = CHILD.replace("</parent>", "<relativePath/></parent>");
    project.documents.replace_all(source.id, &repository_only).unwrap();
    let original = go(property).await.unwrap().pop().unwrap();
    let replaced = format!("<!-- republished -->\n{PARENT}");
    write(&cached, &replaced);
    let updated = go(property).await.unwrap().pop().unwrap();
    assert_ne!(updated.uri, original.uri);
    assert_eq!(updated.range, token(&replaced, 5, "example.version"));
    let editor = ops::doc::doc_open_virtual(&host, "main", updated.uri.unwrap()).await.unwrap();
    assert!(editor.read_only);
    assert_eq!(editor.text, replaced);

    fs::remove_file(&cached).unwrap();
    assert!(go(property).await.unwrap().is_empty());
    assert!(go(dependency).await.unwrap().is_empty());
    let workspace = base.join("workspace");
    ide_workspace::workspace::Workspace::save(&workspace, "Maven workspace", vec![root], true).unwrap();
    host.open_project("workspace", &workspace).unwrap();
    let source = ops::doc::open(&host, "workspace", child).await.unwrap();
    let found = ops::lsp::lsp_goto(&host, "workspace", source.id, property, GotoKind::Definition).await.unwrap();
    assert_eq!(found, [Location::file(pom, token(PARENT, 4, "example.version"))]);
}
