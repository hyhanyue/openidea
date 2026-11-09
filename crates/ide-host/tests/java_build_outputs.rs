//! A real JDT LS build through the same host commands as Run and manual HotSwap.
//! Set OPENIDEA_TEST_JDTLS to the release launcher and OPENIDEA_TEST_JAVA_HOME to its JDK.
//! For offline runs, OPENIDEA_TEST_MAVEN_MIRROR can point to a pre-populated Maven repository.
use ide_core::{LanguageId, RequestId};
use ide_host::{
    Host, HostConfig,
    ops::java::{java_build_workspace, java_resolve_launch},
};
use ide_language::LanguageRegistry;
use ide_lsp::jdtls::JavaBuildStatus;
use ide_workspace::settings::SettingsScope;
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

const WINDOW: &str = "main";
const COORDINATES: &str = "<groupId>demo</groupId><artifactId>reactor</artifactId><version>1.0</version>";
const MAIN: &str = "package demo; public class Main {
    public static void main(String[] args) { System.out.println(Value.text()); }
}";

fn write(root: &Path, name: &str, text: &str) -> PathBuf {
    let file = root.join(name);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, text).unwrap();
    file
}

fn pom(body: &str) -> String {
    format!("<project><modelVersion>4.0.0</modelVersion>{body}</project>")
}

fn fixture(root: &Path) -> (PathBuf, PathBuf, PathBuf) {
    write(
        root,
        "pom.xml",
        &pom(&format!(
            "{COORDINATES}
        <packaging>pom</packaging><modules><module>lib</module><module>app</module></modules>
        <properties><maven.compiler.release>21</maven.compiler.release></properties>
        <build><plugins>
          <plugin><artifactId>maven-clean-plugin</artifactId><version>3.3.2</version></plugin>
          <plugin><artifactId>maven-resources-plugin</artifactId><version>3.3.1</version></plugin>
          <plugin><artifactId>maven-compiler-plugin</artifactId><version>3.13.0</version></plugin>
        </plugins></build>"
        )),
    );
    for module in ["lib", "app"] {
        let dependency = if module == "app" {
            "<dependencies><dependency><groupId>demo</groupId><artifactId>lib</artifactId><version>1.0</version></dependency></dependencies>"
        } else {
            ""
        };
        write(
            root,
            &format!("{module}/pom.xml"),
            &pom(&format!(
                "<parent>{COORDINATES}</parent><artifactId>{module}</artifactId>{dependency}
            <build><outputDirectory>${{project.build.directory}}/custom-main</outputDirectory></build>"
            )),
        );
    }
    (
        write(root, "app/src/main/java/demo/Main.java", MAIN),
        write(root, "lib/src/main/java/demo/Value.java", &value("good-output")),
        write(root, "lib/src/main/java/demo/Gone.java", "package demo; public class Gone {}"),
    )
}

fn value(text: &str) -> String {
    format!("package demo; public class Value {{ public static String text() {{ return \"{text}\"; }} }}")
}

fn run(java: &str, classpath: &std::ffi::OsStr, expected: &str) {
    let output = std::process::Command::new(java).args(["-cp"]).arg(classpath).arg("demo.Main").output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8(output.stdout).unwrap().trim(), expected);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires a JDT LS release and a compatible JDK"]
async fn failed_jdtls_reactor_build_preserves_outputs_and_can_retry() {
    let temp = tempfile::tempdir().unwrap();
    let base = dunce::canonicalize(temp.path()).unwrap();
    let root = base.join("project");
    let java_home = std::env::var("OPENIDEA_TEST_JAVA_HOME").expect("path to the JDT LS JDK");
    let (main, value_source, gone_source) = fixture(&root);
    let new_source = value_source.with_file_name("NewClass.java");
    let mirror = std::env::var("OPENIDEA_TEST_MAVEN_MIRROR")
        .ok()
        .map(|repo| {
            format!(
                "<mirrors><mirror><id>fixture-cache</id><mirrorOf>*</mirrorOf><url>{}</url></mirror></mirrors>",
                ide_lsp::path_to_uri_string(Path::new(&repo))
            )
        })
        .unwrap_or_default();
    let maven_settings = write(
        &base,
        "maven/settings.xml",
        &format!(
            "<settings><localRepository>{}/repository</localRepository>{mirror}</settings>",
            base.join("maven").display()
        ),
    );
    let mut java = LanguageRegistry::load(None).unwrap().get(&LanguageId::new("java")).unwrap();
    let server = &mut java.language_servers[0];
    server.command = std::env::var("OPENIDEA_TEST_JDTLS").expect("path to the JDT LS launcher");
    let settings = &mut server.initialization_options.as_mut().unwrap()["settings"]["java"];
    settings["autobuild"]["enabled"] = false.into();
    settings["import"]["gradle"]["enabled"] = false.into();
    let data = base.join("data");
    let home = base.join("jdtls-home");
    std::fs::create_dir(&home).unwrap();
    server.args.extend([format!("--jvm-arg=-Duser.home={}", home.display()), "--jvm-arg=-Xmx512m".into()]);
    let host = Host::new(HostConfig {
        config_dir: base.join("config"),
        data_dir: data.clone(),
        local_data_dir: data,
        languages: Arc::new(LanguageRegistry::from_configs(vec![java])),
        syntax: Arc::default(),
        sink_for: Box::new(|_| Arc::new(|_| {})),
        owner: |window| window,
        runtime: tokio::runtime::Handle::current(),
        project_environment: false,
    });
    for (key, value) in [
        ("languages.java.jdtlsJdk", json!(java_home)),
        ("languages.java.jdk", json!(java_home)),
        ("languages.java.maven.userSettings", json!(maven_settings)),
    ] {
        host.global_settings.set(SettingsScope::Global, key, value).unwrap();
    }
    host.trust.decide(&root, true, false).unwrap();
    let project = host.open_project(WINDOW, &root).unwrap();
    let module = PathBuf::from("app");
    let launch = || {
        java_resolve_launch(&host, WINDOW, module.clone(), false, ide_run::BuildPolicy::Automatic, RequestId::next())
    };
    // Classpath resolution waits for Maven import even with autobuild disabled.
    java_resolve_launch(&host, WINDOW, module.clone(), false, ide_run::BuildPolicy::NoBuild, RequestId::next())
        .await
        .unwrap();
    let good = launch().await.unwrap();
    assert_eq!(good.build, Some(JavaBuildStatus::Succeeded), "{good:?}");
    let java = good.java_exec.unwrap();
    let classpath = std::env::join_paths(good.class_paths).unwrap();
    let app = root.join("app/target/custom-main/demo/Main.class");
    let lib = root.join("lib/target/custom-main/demo/Value.class");
    let gone = lib.with_file_name("Gone.class");
    let new = lib.with_file_name("NewClass.class");
    let artifacts = [&app, &lib, &gone];
    let contents = || artifacts.map(|path| std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display())));
    let before = contents();
    run(&java, &classpath, "good-output");

    std::fs::write(&main, MAIN.replace("Value.text()", "missing()")).unwrap();
    project.lsp.watched_files_changed(&[], std::slice::from_ref(&main), &[], &[]);
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        // The watched-file notification is processed asynchronously by JDT LS.
        let status = java_build_workspace(&host, WINDOW, module.clone(), None).await.unwrap();
        if status == JavaBuildStatus::WithErrors {
            break;
        }
        assert_eq!(status, JavaBuildStatus::Succeeded);
        assert!(Instant::now() < deadline, "JDT LS did not observe the compile error");
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    assert_eq!(contents(), before);

    // Only change the prerequisite once JDT LS sees the error. A successful
    // build while the first file notification is still pending can publish it.
    std::fs::write(&value_source, value("repaired-output")).unwrap();
    std::fs::remove_file(&gone_source).unwrap();
    std::fs::write(&new_source, "package demo; public class NewClass {}").unwrap();
    project.lsp.watched_files_changed(&[new_source], &[value_source], &[gone_source], &[]);
    let failed = launch().await.unwrap();
    assert_eq!(failed.build, Some(JavaBuildStatus::WithErrors));
    assert_eq!(contents(), before);
    assert!(!new.exists());
    run(&java, &classpath, "good-output");

    drop(project);
    host.close_project(WINDOW);
    std::fs::write(&main, MAIN).unwrap();
    host.open_project(WINDOW, &root).unwrap();
    java_resolve_launch(&host, WINDOW, module.clone(), false, ide_run::BuildPolicy::NoBuild, RequestId::next())
        .await
        .unwrap();
    let recovered = launch().await.unwrap();
    assert_eq!(recovered.build, Some(JavaBuildStatus::Succeeded), "{recovered:?}");
    assert!(new.is_file());
    assert!(!gone.exists());
    run(&java, &classpath, "repaired-output");
    host.shutdown().await;
}
