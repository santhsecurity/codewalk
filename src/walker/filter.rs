use std::collections::HashSet;
use std::path::{Path, PathBuf};

use super::{FileEntry, WalkConfig};
use crate::detect;

pub(crate) fn entry_allowed(path: &Path, root: &Path, config: &WalkConfig) -> bool {
    let depth = symlink_depth(path);
    if !config.follow_symlinks {
        return depth == 0;
    }

    if depth > config.max_symlink_depth {
        return false;
    }

    if depth > 0 && has_symlink_loop(path) {
        return false;
    }

    // Prevent symlink escape outside the scan root.
    if depth > 0 {
        let Ok(canonical) = std::fs::canonicalize(path) else {
            return false;
        };
        let Ok(root_canonical) = std::fs::canonicalize(root) else {
            return false;
        };
        if !canonical.starts_with(&root_canonical) {
            return false;
        }
    }

    true
}

pub(crate) fn process_path(
    path: &Path,
    config: &WalkConfig,
) -> crate::error::Result<Option<FileEntry>> {
    let mut file = std::fs::File::open(path)?;
    let metadata = file.metadata()?;
    let size = metadata.len();

    // Size filter.
    if config.max_file_size > 0 && size > config.max_file_size {
        return Ok(None);
    }

    // Extension filter.
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        let lower = ext.to_ascii_lowercase();
        if !config.include_extensions.is_empty() && !config.include_extensions.contains(&lower) {
            return Ok(None);
        }
        if config.exclude_extensions.contains(&lower) {
            return Ok(None);
        }
    } else if !config.include_extensions.is_empty() {
        // No extension and we have an include filter  -  skip.
        return Ok(None);
    }

    // Binary detection.
    let is_bin = if size == 0 {
        false
    } else {
        detect::is_binary_file(path, &mut file)?
    };
    if config.skip_binary && is_bin {
        return Ok(None);
    }

    Ok(Some(FileEntry {
        path: path.to_path_buf(),
        size,
        is_binary: is_bin,
    }))
}

fn symlink_depth(path: &Path) -> usize {
    let mut depth = 0usize;
    let mut current = PathBuf::new();

    for component in path.components() {
        current.push(component);
        let Ok(metadata) = std::fs::symlink_metadata(&current) else {
            continue;
        };
        if metadata.file_type().is_symlink() {
            depth = depth.saturating_add(1);
        }
    }

    depth
}

fn has_symlink_loop(path: &Path) -> bool {
    let mut seen = HashSet::new();
    let mut current = PathBuf::new();

    for component in path.components() {
        current.push(component);
        let Ok(metadata) = std::fs::symlink_metadata(&current) else {
            continue;
        };
        if !metadata.file_type().is_symlink() {
            continue;
        }

        let Some(identity) = symlink_identity(&current) else {
            continue;
        };
        if !seen.insert(identity) {
            return true;
        }
    }

    false
}

#[cfg(unix)]
#[allow(clippy::unnecessary_wraps)]
fn symlink_identity(path: &Path) -> Option<FileIdentity> {
    use std::os::unix::fs::MetadataExt;

    let metadata = std::fs::symlink_metadata(path).ok()?;
    Some(FileIdentity {
        device: metadata.dev(),
        inode: metadata.ino(),
    })
}

#[cfg(windows)]
#[allow(clippy::unnecessary_wraps)]
fn symlink_identity(path: &Path) -> Option<FileIdentity> {
    use std::os::windows::fs::MetadataExt;

    let metadata = std::fs::symlink_metadata(path).ok()?;
    Some(FileIdentity {
        volume_serial: metadata.volume_serial_number()?.into(),
        file_index: metadata.file_index()?.into(),
    })
}

#[cfg(not(any(unix, windows)))]
fn symlink_identity(_: &Path) -> Option<FileIdentity> {
    None
}

#[cfg(unix)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct FileIdentity {
    device: u64,
    inode: u64,
}

#[cfg(windows)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct FileIdentity {
    volume_serial: u64,
    file_index: u64,
}

#[cfg(not(any(unix, windows)))]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct FileIdentity;

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use crate::walker::test_utils::setup_test_dir;
    use crate::walker::traverse::CodeWalker;
    use std::fs;

    #[cfg(unix)]
    fn symlink_dir(src: &Path, dst: &Path) -> crate::error::Result<()> {
        Ok(std::os::unix::fs::symlink(src, dst)?)
    }

    #[cfg(windows)]
    fn symlink_dir(src: &Path, dst: &Path) -> crate::error::Result<()> {
        Ok(std::os::windows::fs::symlink_dir(src, dst)?)
    }

    fn symlink_enabled_config() -> WalkConfig {
        WalkConfig {
            follow_symlinks: true,
            ..WalkConfig::default()
        }
    }

    #[test]
    fn respects_include_extensions() {
        let dir = setup_test_dir();
        let config = WalkConfig {
            include_extensions: ["rs"]
                .iter()
                .map(std::string::ToString::to_string)
                .collect(),
            ..WalkConfig::default()
        };
        let walker = CodeWalker::new(dir.path(), config);
        let entries = walker.walk().unwrap();
        assert!(entries.iter().all(|e| e.path.extension().unwrap() == "rs"));
    }

    #[test]
    fn respects_exclude_extensions() {
        let dir = setup_test_dir();
        let config = WalkConfig {
            exclude_extensions: ["py"]
                .iter()
                .map(std::string::ToString::to_string)
                .collect(),
            ..WalkConfig::default()
        };
        let walker = CodeWalker::new(dir.path(), config);
        let entries = walker.walk().unwrap();
        assert!(entries.iter().all(|e| e.path.extension().unwrap() != "py"));
    }

    #[test]
    fn respects_max_file_size() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("small.txt"), "hi").unwrap();
        fs::write(dir.path().join("big.txt"), "x".repeat(1000)).unwrap();

        let config = WalkConfig {
            max_file_size: 100,
            skip_binary: false,
            ..WalkConfig::default()
        };
        let walker = CodeWalker::new(dir.path(), config);
        let entries = walker.walk().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path.file_name().unwrap(), "small.txt");
    }

    #[test]
    fn includes_binary_when_not_skipped() {
        let dir = setup_test_dir();
        let config = WalkConfig {
            skip_binary: false,
            ..WalkConfig::default()
        };
        let walker = CodeWalker::new(dir.path(), config);
        let entries = walker.walk().unwrap();
        let has_bin = entries
            .iter()
            .any(|e| e.path.file_name().unwrap() == "data.bin");
        assert!(has_bin);
    }

    #[test]
    fn follows_symlinks_when_enabled() {
        let dir = tempfile::tempdir().unwrap();
        let real_dir = dir.path().join("real");
        fs::create_dir(&real_dir).unwrap();
        fs::write(real_dir.join("inside.txt"), "linked").unwrap();

        let linked_dir = dir.path().join("linked");
        symlink_dir(&real_dir, &linked_dir).unwrap();

        let linked_inside = linked_dir.join("inside.txt");

        let entries = CodeWalker::new(dir.path(), WalkConfig::default())
            .walk()
            .unwrap();
        assert!(!entries.iter().any(|entry| entry.path == linked_inside));

        let entries = CodeWalker::new(dir.path(), symlink_enabled_config())
            .walk()
            .unwrap();
        assert!(entries.iter().any(|entry| entry.path == linked_inside));
    }

    #[test]
    fn handles_empty_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty.txt");
        fs::write(&path, b"").unwrap();

        let walker = CodeWalker::new(dir.path(), WalkConfig::default());
        let entries = walker.walk().unwrap();
        let entry = entries.iter().find(|entry| entry.path == path);
        assert!(entry.is_some());
        let entry = entry.unwrap();
        assert_eq!(entry.size, 0);
        assert!(!entry.is_binary);
    }
}
