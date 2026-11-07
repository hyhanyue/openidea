//! Defaults shipped by the personal fork; resources come from the signed app bundle.
use crate::plugins::{Consent, PluginHost, PluginSource};
use ide_core::IdeResult;
use ide_core::settings::Settings;
use ide_language::LanguageServerConfig;
use serde_json::json;
use std::path::{Path, PathBuf};

const PACKAGES: [(&str, &str, &str); 2] = [
    ("mybatis", "hyhanyue.mybatis", "hyhanyue.mybatis-local"),
    ("spring-beans", "hyhanyue.spring-beans", "hyhanyue.spring-beans-local"),
];

pub fn bind_resources(data: &Path, resources: &Path) -> IdeResult<()> {
    if resources.join("manifest.json").is_file() {
        std::fs::create_dir_all(data.join("fork-defaults"))?;
        ide_vfs::write_atomic(&data.join("fork-defaults/resource-root"), resources.to_string_lossy().as_bytes())?;
    }
    Ok(())
}

fn resources(data: &Path) -> Option<PathBuf> {
    let path = PathBuf::from(std::fs::read_to_string(data.join("fork-defaults/resource-root")).ok()?);
    path.join("manifest.json").is_file().then_some(path)
}

fn copy_tree(source: &Path, target: &Path) -> IdeResult<()> {
    std::fs::create_dir_all(target)?;
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let destination = target.join(entry.file_name());
        if ty.is_dir() {
            copy_tree(&entry.path(), &destination)?;
        } else if ty.is_file() {
            std::fs::copy(entry.path(), destination)?;
        }
    }
    Ok(())
}

pub fn install_plugins(host: &PluginHost, data: &Path) -> IdeResult<()> {
    let Some(resources) = resources(data) else { return Ok(()) };
    let marker = data.join("fork-defaults/plugins-initialized");
    let first = !marker.is_file();
    for (folder, id, old_id) in PACKAGES {
        let source = resources.join("plugins").join(folder);
        let manifest = ide_extension::Manifest::load(&source).map_err(ide_core::IdeError::invalid)?;
        let existing = host.info_of(id).ok();
        if existing.as_ref().is_some_and(|p| p.version == manifest.version) || !first && existing.is_none() {
            continue;
        }
        let stage = data.join("fork-defaults/staging").join(folder);
        if stage.exists() {
            std::fs::remove_dir_all(&stage)?;
        }
        copy_tree(&source, &stage)?;
        // Keep this Mac's personal icon overrides out of the public release assets.
        let overrides = data.join("fork-defaults/personal-icons").join(folder);
        if first && let Ok(old) = host.info_of(old_id) {
            if folder == "spring-beans" && old.path.join("icons").is_dir() {
                copy_tree(&old.path.join("icons"), &overrides)?;
            }
            if old.source == PluginSource::Linked {
                host.unlink(old_id)?;
            } else {
                host.set_enabled(old_id, false)?;
            }
        }
        if overrides.is_dir() {
            copy_tree(&overrides, &stage.join("icons"))?;
        }
        let enabled = first || existing.as_ref().is_some_and(|p| p.enabled);
        host.install_from_dir(&stage, &Consent { optional: vec!["events".into()], signer_change: false })?;
        host.set_enabled(id, enabled)?;
    }
    if first {
        for id in ["aritti.spring-beans", "aritti.spring-boot", "openidea.mybatis"] {
            if host.info_of(id).is_ok() {
                host.set_enabled(id, false)?;
            }
        }
        std::fs::write(marker, b"1\n")?;
    }
    Ok(())
}

pub fn spring_project(root: &Path) -> bool {
    ["pom.xml", "build.gradle", "build.gradle.kts"]
        .iter()
        .any(|name| std::fs::read_to_string(root.join(name)).is_ok_and(|s| s.contains("org.springframework.boot")))
}

pub(crate) fn spring_bundles(data: &Path, root: &Path) -> Vec<PathBuf> {
    if !spring_project(root) {
        return Vec::new();
    }
    resources(data)
        .and_then(|r| std::fs::read_dir(r.join("spring-tools/extension/jars")).ok())
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "jar"))
        .collect()
}

pub(crate) fn add_spring_server(
    data: &Path,
    root: &Path,
    language: &str,
    settings: &Settings,
    servers: &mut Vec<LanguageServerConfig>,
) {
    if !cfg!(target_os = "macos")
        || !["java", "properties", "yaml"].contains(&language)
        || settings.language_servers.contains_key("spring-boot")
        || servers.iter().any(|s| s.name == "spring-boot")
        || !spring_project(root)
    {
        return;
    }
    let Some(resources) = resources(data) else { return };
    let python = ["/opt/homebrew/bin/python3", "/usr/local/bin/python3", "/usr/bin/python3"]
        .into_iter()
        .find(|p| Path::new(p).is_file());
    let Some(python) = python else { return };
    let jdks = ide_sdk::java::jdtls_jdks(21, data);
    let Some(runtime) = ide_sdk::java::jdtls_runtime(&jdks, 21) else { return };
    let jar =
        resources.join("spring-tools/extension/language-server/spring-boot-language-server-2.3.0-SNAPSHOT-exec.jar");
    if !jar.is_file() {
        return;
    }
    let spec = data.join("fork-defaults/profiles").join(crate::project::project_key(root)).join("spring.json");
    let value = json!({"engine":"spring", "project":root, "command":[runtime.home.join("bin/java"),
        "-Dspring.config.location=classpath:/application.properties", "-Dserver.port=-1", "-Xmx1g", "-jar", jar]});
    let write = || -> IdeResult<()> {
        std::fs::create_dir_all(spec.parent().unwrap())?;
        let bytes = serde_json::to_vec_pretty(&value).map_err(|e| ide_core::IdeError::internal(e.to_string()))?;
        if std::fs::read(&spec).ok().as_deref() != Some(bytes.as_slice()) {
            ide_vfs::write_atomic(&spec, &bytes)?;
        }
        Ok(())
    };
    if let Err(error) = write() {
        tracing::warn!(%error, "cannot prepare bundled Spring service");
        return;
    }
    let config = json!({"name":"spring-boot", "command":python,
        "args":[resources.join("bin/launch.py"), spec], "root-markers":["pom.xml", "build.gradle", "build.gradle.kts"],
        "require-root-marker":true, "features":["completion", "hover", "definition", "references", "code-action", "diagnostics"],
        "initialization-options":{"enableJdtClasspath":true, "workspaceFolders":[ide_lsp::path_to_uri_string(root)]}});
    match serde_json::from_value(config) {
        Ok(config) => servers.push(config),
        Err(error) => tracing::warn!(%error, "invalid bundled Spring configuration"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn plain_java_does_not_load_spring_and_explicit_choices_win() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("pom.xml"), "<project/>").unwrap();
        assert!(!spring_project(temp.path()));
        let mut servers = Vec::new();
        add_spring_server(temp.path(), temp.path(), "java", &Settings::default(), &mut servers);
        assert!(servers.is_empty());
        std::fs::write(temp.path().join("pom.xml"), "<groupId>org.springframework.boot</groupId>").unwrap();
        assert!(spring_project(temp.path()));
        let mut settings = Settings::default();
        settings.language_servers.insert("spring-boot".into(), Default::default());
        add_spring_server(temp.path(), temp.path(), "java", &settings, &mut servers);
        assert!(servers.is_empty());
    }
}
