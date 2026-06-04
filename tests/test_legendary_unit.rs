#![allow(clippy::unwrap_used)]

use codewalk::detect::is_binary;
use codewalk::{CodeWalker, WalkConfig};
use std::collections::HashSet;
use std::fs;
use tempfile::TempDir;

fn setup_test_dir() -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("main.rs"), "fn main() {}").unwrap();
    fs::write(dir.path().join("lib.rs"), "pub fn hello() {}").unwrap();
    fs::write(
        dir.path().join("data.bin"),
        b"\x7fELF\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00",
    )
    .unwrap();
    fs::create_dir(dir.path().join("node_modules")).unwrap();
    fs::write(dir.path().join("node_modules/junk.js"), "// junk").unwrap();
    fs::create_dir(dir.path().join("src")).unwrap();
    fs::write(dir.path().join("src/app.py"), "print('hello')").unwrap();
    dir
}

#[test]
fn test_walk_config_builders() {
    let mut ext = HashSet::new();
    ext.insert("rs".to_string());

    let config = WalkConfig::builder()
        .max_file_size(1024)
        .skip_binary(false)
        .skip_hidden(false)
        .respect_gitignore(false)
        .follow_symlinks(true)
        .include_extensions(ext.clone())
        .exclude_extensions(ext.clone())
        .exclude_dirs(ext.clone())
        .ignore_files(vec![".myignore".to_string()])
        .ignore_patterns(vec!["*.tmp".to_string()])
        .max_symlink_depth(42);

    assert_eq!(config.max_file_size, 1024);
    assert!(!config.skip_binary);
    assert!(!config.skip_hidden);
    assert!(!config.respect_gitignore);
    assert!(config.follow_symlinks);
    assert_eq!(config.include_extensions.len(), 1);
    assert_eq!(config.exclude_extensions.len(), 1);
    assert_eq!(config.exclude_dirs.len(), 1);
    assert_eq!(config.ignore_files, vec![".myignore".to_string()]);
    assert_eq!(config.ignore_patterns, vec!["*.tmp".to_string()]);
    assert_eq!(config.max_symlink_depth, 42);
}

#[test]
fn test_walk_config_load() {
    let dir = tempfile::tempdir().unwrap();
    let toml_path = dir.path().join("config.toml");
    fs::write(
        &toml_path,
        r"
        max_file_size = 999
        skip_binary = false
    ",
    )
    .unwrap();

    let config = WalkConfig::load(&toml_path).unwrap();
    assert_eq!(config.max_file_size, 999);
    assert!(!config.skip_binary);
}

#[test]
fn test_walk_config_from_toml() {
    let config = WalkConfig::from_toml(
        r"
        max_file_size = 999
        skip_binary = false
    ",
    )
    .unwrap();
    assert_eq!(config.max_file_size, 999);
    assert!(!config.skip_binary);
}

#[test]
fn test_walk_config_artifact_defaults() {
    let config = WalkConfig::artifact_defaults();
    assert!(!config.skip_hidden);
    assert!(!config.respect_gitignore);
    assert!(config.exclude_dirs.is_empty());
}

#[test]
fn test_codewalk_walk_sorted() {
    let dir = setup_test_dir();
    let walker = CodeWalker::new(dir.path(), WalkConfig::default());
    let entries = walker.walk_sorted().unwrap();

    assert!(entries.len() >= 2);
    for i in 0..entries.len() - 1 {
        assert!(entries[i].path <= entries[i + 1].path);
    }
}

#[test]
fn test_codewalk_count() {
    let dir = setup_test_dir();
    let walker = CodeWalker::new(dir.path(), WalkConfig::default());
    let count = walker.count();
    let entries = walker.walk().unwrap();
    assert_eq!(count, entries.len());
    assert!(count > 0);
}

#[test]
fn test_file_entry_methods() {
    let dir = setup_test_dir();
    let walker = CodeWalker::new(dir.path(), WalkConfig::default());
    let entries = walker.walk().unwrap();

    let main_rs = entries
        .iter()
        .find(|e| e.path.file_name().unwrap() == "main.rs")
        .unwrap();

    let chunks: Vec<_> = main_rs
        .content_chunks()
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let mut all_bytes = Vec::new();
    for chunk in chunks {
        all_bytes.extend(chunk);
    }
    assert_eq!(all_bytes, b"fn main() {}");

    let content = main_rs.content().unwrap();
    assert!(content.is_text());
    assert!(!content.is_binary());
    assert!(!content.is_unknown());
    assert_eq!(content.len(), 12);
    assert!(!content.is_empty());
    assert_eq!(content.as_text().unwrap(), "fn main() {}");

    let s = main_rs.content_str().unwrap();
    assert_eq!(s, "fn main() {}");
}

#[test]
fn test_detect_is_binary() {
    let dir = tempfile::tempdir().unwrap();
    let bin_path = dir.path().join("test.exe");
    fs::write(
        &bin_path,
        b"MZ\x90\x00\x03\x00\x00\x00\x04\x00\x00\x00\xFF\xFF\x00\x00",
    )
    .unwrap();

    let is_bin = is_binary(&bin_path).unwrap();
    assert!(is_bin);

    let text_path = dir.path().join("test.txt");
    fs::write(&text_path, b"hello world").unwrap();
    let is_bin = is_binary(&text_path).unwrap();
    assert!(!is_bin);
}

#[test]
fn test_file_content_unknown() {
    let dir = tempfile::tempdir().unwrap();
    let bad_path = dir.path().join("bad.txt");
    fs::write(&bad_path, b"hello \xFF world").unwrap();

    let config = WalkConfig::default().skip_binary(false);
    let walker = CodeWalker::new(dir.path(), config);
    let entries = walker.walk().unwrap();
    let entry = entries.into_iter().next().unwrap();

    let content = entry.content().unwrap();
    assert!(content.is_unknown());
    assert_eq!(content.as_bytes(), b"hello \xFF world");

    let res = entry.content_str();
    assert!(res.is_err());
}
