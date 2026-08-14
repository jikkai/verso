use std::{
    collections::{HashMap, HashSet},
    fs,
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Debug)]
pub struct ChangeSet {
    originals: HashMap<PathBuf, Option<Vec<u8>>>,
    written: Vec<PathBuf>,
    written_paths: HashSet<PathBuf>,
}

impl ChangeSet {
    pub fn snapshot(paths: &[PathBuf]) -> Result<Self, String> {
        let mut originals = HashMap::new();

        for path in paths {
            originals.insert(path.clone(), original_contents(path)?);
        }

        Ok(Self {
            originals,
            written: Vec::new(),
            written_paths: HashSet::new(),
        })
    }

    pub fn write(&mut self, path: &Path, contents: &[u8]) -> Result<(), String> {
        if !self.originals.contains_key(path) {
            self.originals
                .insert(path.to_path_buf(), original_contents(path)?);
        }

        atomic_write(path, contents)
            .map_err(|error| format!("failed to write {}: {error}", path.display()))?;

        let written_path = path.to_path_buf();
        if self.written_paths.insert(written_path.clone()) {
            self.written.push(written_path);
        }

        Ok(())
    }

    /// Restores written paths in reverse write order.
    ///
    /// Attempts every path and reports all restore/remove failures together.
    pub fn rollback(&self) -> Result<Vec<PathBuf>, String> {
        let mut restored = Vec::new();
        let mut errors = Vec::new();

        for path in self.written.iter().rev() {
            let result = match self.originals.get(path) {
                Some(Some(contents)) => atomic_write(path, contents)
                    .map_err(|error| format!("failed to restore {}: {error}", path.display())),
                Some(None) => match fs::remove_file(path) {
                    Ok(()) => sync_parent(
                        path.parent()
                            .filter(|parent| !parent.as_os_str().is_empty())
                            .unwrap_or_else(|| Path::new(".")),
                    ),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                    Err(error) => Err(format!("failed to remove {}: {error}", path.display())),
                },
                None => Err(format!("missing rollback snapshot for {}", path.display())),
            };

            match result {
                Ok(()) => restored.push(path.clone()),
                Err(error) => errors.push(error),
            }
        }

        if errors.is_empty() {
            Ok(restored)
        } else {
            Err(errors.join("; "))
        }
    }
}

pub(crate) fn atomic_write(path: &Path, contents: &[u8]) -> Result<(), String> {
    create_parent_dir(path)?;

    let permissions = match fs::metadata(path) {
        Ok(metadata) => Some(metadata.permissions()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(format!("failed to read file permissions: {error}")),
    };
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut temporary = tempfile::NamedTempFile::new_in(parent)
        .map_err(|error| format!("failed to create temporary file: {error}"))?;

    temporary
        .write_all(contents)
        .map_err(|error| format!("failed to write temporary file: {error}"))?;
    temporary
        .flush()
        .map_err(|error| format!("failed to flush temporary file: {error}"))?;
    if let Some(permissions) = permissions {
        temporary
            .as_file()
            .set_permissions(permissions)
            .map_err(|error| format!("failed to preserve file permissions: {error}"))?;
    }
    temporary
        .as_file()
        .sync_all()
        .map_err(|error| format!("failed to sync temporary file: {error}"))?;
    temporary
        .persist(path)
        .map_err(|error| format!("failed to replace destination: {}", error.error))?;
    sync_parent(parent)?;

    Ok(())
}

#[cfg(unix)]
fn sync_parent(parent: &Path) -> Result<(), String> {
    fs::File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| format!("failed to sync parent directory: {error}"))
}

#[cfg(not(unix))]
fn sync_parent(_parent: &Path) -> Result<(), String> {
    Ok(())
}

fn original_contents(path: &Path) -> Result<Option<Vec<u8>>, String> {
    match fs::read(path) {
        Ok(contents) => Ok(Some(contents)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("failed to snapshot {}: {error}", path.display())),
    }
}

fn create_parent_dir(path: &Path) -> Result<(), String> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };

    if parent.as_os_str().is_empty() {
        return Ok(());
    }

    fs::create_dir_all(parent)
        .map_err(|error| format!("failed to create {}: {error}", parent.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn restores_a_written_existing_file() -> Result<(), String> {
        let temp = TempDir::new().map_err(|error| error.to_string())?;
        let path = temp.path().join("package.json");
        fs::write(&path, b"original").map_err(|error| error.to_string())?;
        let mut changes = ChangeSet::snapshot(std::slice::from_ref(&path))?;

        changes.write(&path, b"changed")?;
        let restored = changes.rollback()?;

        assert_eq!(
            fs::read(&path).map_err(|error| error.to_string())?,
            b"original"
        );
        assert_eq!(restored, vec![path]);
        Ok(())
    }

    #[test]
    fn removes_a_created_file() -> Result<(), String> {
        let temp = TempDir::new().map_err(|error| error.to_string())?;
        let path = temp.path().join("generated/package.json");
        let mut changes = ChangeSet::snapshot(std::slice::from_ref(&path))?;

        changes.write(&path, b"created")?;
        let restored = changes.rollback()?;

        assert!(!path.exists());
        assert_eq!(restored, vec![path]);
        Ok(())
    }

    #[test]
    fn writing_the_same_file_more_than_once_restores_the_original() -> Result<(), String> {
        let temp = TempDir::new().map_err(|error| error.to_string())?;
        let path = temp.path().join("package.json");
        fs::write(&path, b"original").map_err(|error| error.to_string())?;
        let mut changes = ChangeSet::snapshot(std::slice::from_ref(&path))?;

        changes.write(&path, b"first change")?;
        changes.write(&path, b"second change")?;
        let restored = changes.rollback()?;

        assert_eq!(
            fs::read(&path).map_err(|error| error.to_string())?,
            b"original"
        );
        assert_eq!(restored, vec![path]);
        Ok(())
    }

    #[test]
    fn restores_multiple_files_in_reverse_write_order() -> Result<(), String> {
        let temp = TempDir::new().map_err(|error| error.to_string())?;
        let a = temp.path().join("a.txt");
        let b = temp.path().join("b.txt");
        fs::write(&a, b"a original").map_err(|error| error.to_string())?;
        fs::write(&b, b"b original").map_err(|error| error.to_string())?;
        let mut changes = ChangeSet::snapshot(&[a.clone(), b.clone()])?;

        changes.write(&a, b"a changed")?;
        changes.write(&b, b"b changed")?;
        let restored = changes.rollback()?;

        assert_eq!(restored, vec![b, a]);
        Ok(())
    }

    #[test]
    fn rollback_only_affects_paths_that_were_written() -> Result<(), String> {
        let temp = TempDir::new().map_err(|error| error.to_string())?;
        let written = temp.path().join("written.txt");
        let unwritten = temp.path().join("unwritten.txt");
        fs::write(&written, b"written original").map_err(|error| error.to_string())?;
        fs::write(&unwritten, b"unwritten original").map_err(|error| error.to_string())?;
        let mut changes = ChangeSet::snapshot(&[written.clone(), unwritten.clone()])?;

        changes.write(&written, b"written changed")?;
        fs::write(&unwritten, b"outside change").map_err(|error| error.to_string())?;
        let restored = changes.rollback()?;

        assert_eq!(
            fs::read(&written).map_err(|error| error.to_string())?,
            b"written original"
        );
        assert_eq!(
            fs::read(&unwritten).map_err(|error| error.to_string())?,
            b"outside change"
        );
        assert_eq!(restored, vec![written]);
        Ok(())
    }

    #[test]
    fn preserves_binary_content() -> Result<(), String> {
        let temp = TempDir::new().map_err(|error| error.to_string())?;
        let path = temp.path().join("artifact.bin");
        let original = vec![0, 159, 146, 150, 255];
        fs::write(&path, &original).map_err(|error| error.to_string())?;
        let mut changes = ChangeSet::snapshot(std::slice::from_ref(&path))?;

        changes.write(&path, &[1, 2, 3, 4])?;
        changes.rollback()?;

        assert_eq!(
            fs::read(&path).map_err(|error| error.to_string())?,
            original
        );
        Ok(())
    }

    #[test]
    fn rollback_continues_after_a_restore_failure() -> Result<(), String> {
        let temp = TempDir::new().map_err(|error| error.to_string())?;
        let restored_path = temp.path().join("restored.txt");
        let blocked_path = temp.path().join("blocked.txt");
        fs::write(&restored_path, b"restored original").map_err(|error| error.to_string())?;
        fs::write(&blocked_path, b"blocked original").map_err(|error| error.to_string())?;
        let mut changes = ChangeSet::snapshot(&[restored_path.clone(), blocked_path.clone()])?;

        changes.write(&restored_path, b"restored changed")?;
        changes.write(&blocked_path, b"blocked changed")?;
        fs::remove_file(&blocked_path).map_err(|error| error.to_string())?;
        fs::create_dir(&blocked_path).map_err(|error| error.to_string())?;

        let error = changes
            .rollback()
            .expect_err("replacing a directory with a file should fail");

        assert!(error.contains("failed to restore"));
        assert_eq!(
            fs::read(&restored_path).map_err(|error| error.to_string())?,
            b"restored original"
        );
        assert!(blocked_path.is_dir());
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn write_preserves_existing_permissions() -> Result<(), String> {
        use std::os::unix::fs::PermissionsExt;

        let temp = TempDir::new().map_err(|error| error.to_string())?;
        let path = temp.path().join("executable.sh");
        fs::write(&path, b"old").map_err(|error| error.to_string())?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o744))
            .map_err(|error| error.to_string())?;
        let mut changes = ChangeSet::snapshot(std::slice::from_ref(&path))?;

        changes.write(&path, b"new")?;

        let mode = fs::metadata(&path)
            .map_err(|error| error.to_string())?
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o744);
        Ok(())
    }
}
