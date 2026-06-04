#![allow(clippy::unwrap_used)]

use codewalk::{CodeWalker, FileEntry, WalkConfig};
use std::fs;
use std::path::Path;

#[cfg(unix)]
use std::os::unix::ffi::OsStringExt;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

#[cfg(unix)]
fn symlink_dir(src: &Path, dst: &Path) -> codewalk::error::Result<()> {
    Ok(std::os::unix::fs::symlink(src, dst)?)
}

#[cfg(windows)]
fn symlink_dir(src: &Path, dst: &Path) -> codewalk::error::Result<()> {
    Ok(std::os::windows::fs::symlink_dir(src, dst)?)
}

fn symlink_enabled_config() -> WalkConfig {
    WalkConfig::builder().follow_symlinks(true)
}

fn setup_test_dir() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("main.rs"), "fn main() {}").unwrap();
    fs::write(dir.path().join("lib.rs"), "pub fn hello() {}").unwrap();
    fs::write(dir.path().join("data.bin"), b"\x7fELF\x00\x00\x00\x00").unwrap();
    fs::create_dir(dir.path().join("node_modules")).unwrap();
    fs::write(dir.path().join("node_modules/junk.js"), "// junk").unwrap();
    fs::create_dir(dir.path().join("src")).unwrap();
    fs::write(dir.path().join("src/app.py"), "print('hello')").unwrap();
    dir
}

#[test]
fn walks_directory() {
    let dir = setup_test_dir();
    let walker = CodeWalker::new(dir.path(), WalkConfig::default());
    let entries = walker.walk().unwrap();
    // Should find main.rs, lib.rs, src/app.py (not data.bin, not node_modules/)
    assert!(entries.len() >= 2);
    let paths: Vec<String> = entries
        .iter()
        .map(|e| e.path.file_name().unwrap().to_string_lossy().to_string())
        .collect();
    assert!(paths.contains(&"main.rs".to_string()));
    assert!(paths.contains(&"lib.rs".to_string()));
    assert!(!paths.contains(&"data.bin".to_string())); // binary skipped
    assert!(!paths.contains(&"junk.js".to_string())); // node_modules skipped
}

#[test]
fn respects_include_extensions() {
    let dir = setup_test_dir();
    let config = WalkConfig::builder().include_extensions(
        ["rs"]
            .iter()
            .map(std::string::ToString::to_string)
            .collect(),
    );
    let walker = CodeWalker::new(dir.path(), config);
    let entries = walker.walk().unwrap();
    assert!(entries.iter().all(|e| e.path.extension().unwrap() == "rs"));
}

#[test]
fn respects_exclude_extensions() {
    let dir = setup_test_dir();
    let config = WalkConfig::builder().exclude_extensions(
        ["py"]
            .iter()
            .map(std::string::ToString::to_string)
            .collect(),
    );
    let walker = CodeWalker::new(dir.path(), config);
    let entries = walker.walk().unwrap();
    assert!(entries.iter().all(|e| e.path.extension().unwrap() != "py"));
}

#[test]
fn respects_max_file_size() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("small.txt"), "hi").unwrap();
    fs::write(dir.path().join("big.txt"), "x".repeat(1000)).unwrap();

    let config = WalkConfig::builder().max_file_size(100).skip_binary(false);
    let walker = CodeWalker::new(dir.path(), config);
    let entries = walker.walk().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].path.file_name().unwrap(), "small.txt");
}

#[test]
fn includes_binary_when_not_skipped() {
    let dir = setup_test_dir();
    let config = WalkConfig::builder().skip_binary(false);
    let walker = CodeWalker::new(dir.path(), config);
    let entries = walker.walk().unwrap();
    let has_bin = entries
        .iter()
        .any(|e| e.path.file_name().unwrap() == "data.bin");
    assert!(has_bin);
}

#[test]
fn file_content_read() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("test.txt"), "hello world").unwrap();

    let config = WalkConfig::builder().skip_binary(false);
    let walker = CodeWalker::new(dir.path(), config);
    let entries = walker.walk().unwrap();
    assert_eq!(entries.len(), 1);

    let content = entries[0].content().unwrap();
    assert_eq!(content.as_bytes(), b"hello world");
    assert_eq!(content.len(), 11);
    assert!(!content.is_empty());
}

#[test]
fn file_content_str() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("test.rs"), "fn main() {}").unwrap();

    let walker = CodeWalker::new(dir.path(), WalkConfig::default());
    let entries = walker.walk().unwrap();
    let s = entries[0].content_str().unwrap();
    assert_eq!(s, "fn main() {}");
}

#[test]
fn parallel_walk() {
    let dir = setup_test_dir();
    let walker = CodeWalker::new(dir.path(), WalkConfig::default());
    let rx = walker.walk_parallel(2);
    let entries: Vec<FileEntry> = rx.iter().collect::<Result<Vec<_>, _>>().unwrap();
    assert!(entries.len() >= 2);
}

#[test]
fn empty_directory() {
    let dir = tempfile::tempdir().unwrap();
    let walker = CodeWalker::new(dir.path(), WalkConfig::default());
    let entries = walker.walk().unwrap();
    assert!(entries.is_empty());
}

#[test]
fn count_matches_walk() {
    let dir = setup_test_dir();
    let walker = CodeWalker::new(dir.path(), WalkConfig::default());
    let count = walker.count();
    let entries = walker.walk().unwrap();
    assert_eq!(count, entries.len());
}

#[test]
fn default_config_excludes_common_dirs() {
    let config = WalkConfig::default();
    assert!(config.exclude_dirs.contains("node_modules"));
    assert!(config.exclude_dirs.contains(".git"));
    assert!(config.exclude_dirs.contains("target"));
    assert!(config.exclude_dirs.contains("__pycache__"));
    assert!(config.exclude_dirs.contains("vendor"));
}

#[test]
fn walk_iter_collects_entries() {
    let dir = setup_test_dir();
    let walker = CodeWalker::new(dir.path(), WalkConfig::default());
    let entries: Vec<FileEntry> = walker.walk_iter().collect::<Result<Vec<_>, _>>().unwrap();
    let paths: Vec<&Path> = entries.iter().map(|entry| entry.path.as_path()).collect();
    assert!(paths.iter().any(|p| p.ends_with("main.rs")));
    assert!(paths.iter().any(|p| p.ends_with("lib.rs")));
    assert!(paths.iter().any(|p| p.ends_with("src/app.py")));
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

#[cfg(unix)]
#[test]
fn handles_non_utf8_filenames() {
    use std::ffi::OsString;

    let dir = tempfile::tempdir().unwrap();
    let invalid_name = {
        let mut raw = b"bad-".to_vec();
        raw.extend_from_slice(b"\xffname.txt");
        OsString::from_vec(raw)
    };
    let path = dir.path().join(&invalid_name);
    fs::write(&path, "unicode").unwrap();

    let walker = CodeWalker::new(dir.path(), WalkConfig::default());
    let entries = walker.walk().unwrap();
    assert!(entries.iter().any(|entry| entry.path == path));
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

#[cfg(unix)]
#[test]
fn handles_permission_denied() {
    let dir = tempfile::tempdir().unwrap();
    let public_file = dir.path().join("public.txt");
    fs::write(&public_file, "allowed").unwrap();

    let blocked_dir = dir.path().join("blocked");
    fs::create_dir(&blocked_dir).unwrap();
    let blocked_file = blocked_dir.join("secret.txt");
    fs::write(&blocked_file, "secret").unwrap();

    let original_permissions = fs::metadata(&blocked_dir).unwrap().permissions();
    let mut blocked_permissions = original_permissions.clone();
    blocked_permissions.set_mode(0o000);
    fs::set_permissions(&blocked_dir, blocked_permissions).unwrap();

    let can_read_blocked_dir = fs::read_dir(&blocked_dir).is_ok();

    let results: Vec<_> = CodeWalker::new(dir.path(), WalkConfig::default())
        .walk_iter()
        .collect();
    let _ = fs::set_permissions(&blocked_dir, original_permissions);

    let entries: Vec<_> = results
        .iter()
        .filter_map(|result| result.as_ref().ok())
        .collect();
    assert!(entries.iter().any(|entry| entry.path == public_file));
    if !can_read_blocked_dir {
        assert!(
            !entries
                .iter()
                .any(|entry| entry.path.starts_with(&blocked_dir))
        );
    }
}
