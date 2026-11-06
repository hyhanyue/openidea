//! Roll back previously published Maven classes after a failed implicit build.
use super::fingerprint::content_hash;
use ide_core::{CancellationToken, IdeError, IdeResult};
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

pub(super) struct OutputBackup {
    output: PathBuf,
    backup: TempDir,
}

pub(super) fn backup(
    modules: impl IntoIterator<Item = PathBuf>,
    cancel: &CancellationToken,
) -> IdeResult<Vec<OutputBackup>> {
    let mut backups = Vec::new();
    for module in modules {
        let target = module.join("target");
        if !is_directory(&target)? {
            continue;
        }
        for name in ["classes", "test-classes"] {
            if cancel.is_cancelled() {
                return Err(IdeError::cancelled());
            }
            let output = target.join(name);
            if is_directory(&output)? {
                // Keep the copy outside target so Maven clean cannot delete it, on the
                // same filesystem so rollback can move a complete directory into place.
                let backup = tempfile::Builder::new().prefix(".openidea-launch-").tempdir_in(&module)?;
                ide_vfs::copy(&output, &backup.path().join("previous"))?;
                backups.push(OutputBackup { output, backup });
            }
        }
    }
    Ok(backups)
}

fn is_directory(path: &Path) -> std::io::Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(meta) => Ok(meta.is_dir()),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e),
    }
}

impl OutputBackup {
    pub(super) fn restore(self) -> IdeResult<()> {
        if let Err(error) = self.restore_inner() {
            let backup = self.backup.keep();
            return Err(IdeError::internal(format!(
                "Cannot restore {}: {error}. Previous output retained at {}",
                self.output.display(),
                backup.join("previous").display()
            )));
        }
        Ok(())
    }

    fn restore_inner(&self) -> IdeResult<()> {
        let target = self.output.parent().expect("Maven output has a target parent");
        fs::create_dir_all(target)?;
        if !is_directory(target)? {
            return Err(IdeError::invalid(format!("Maven target is no longer a directory: {}", target.display())));
        }
        let previous = self.backup.path().join("previous");
        // The model's reactor can include projects Maven skipped. Don't rewrite
        // unchanged outputs, including after failure to start the Maven process.
        match same_tree(&self.output, &previous) {
            Ok(true) => return Ok(()),
            Ok(false) => {}
            Err(e) if e.kind == ide_core::error::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        fs::symlink_metadata(&previous)?;
        let partial = self.backup.path().join("partial");
        let had_partial = match fs::rename(&self.output, &partial) {
            Ok(()) => true,
            Err(e) if e.kind() == ErrorKind::NotFound => false,
            Err(e) => return Err(e.into()),
        };
        if let Err(error) = fs::rename(&previous, &self.output) {
            if had_partial {
                let _ = fs::rename(&partial, &self.output);
            }
            return Err(error.into());
        }
        Ok(())
    }
}

fn same_tree(path: &Path, backup: &Path) -> IdeResult<bool> {
    let live = fs::symlink_metadata(path)?;
    let saved = fs::symlink_metadata(backup)?;
    if live.file_type() != saved.file_type() {
        return Ok(false);
    }
    if live.is_dir() {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            if !same_tree(&entry.path(), &backup.join(entry.file_name()))? {
                return Ok(false);
            }
        }
        for entry in fs::read_dir(backup)? {
            fs::symlink_metadata(path.join(entry?.file_name()))?;
        }
        return Ok(true);
    }
    if live.file_type().is_symlink() {
        return Ok(fs::read_link(path)? == fs::read_link(backup)?);
    }
    Ok(live.is_file() && live.len() == saved.len() && content_hash(path)? == content_hash(backup)?)
}
