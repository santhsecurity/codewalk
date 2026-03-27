use crate::error::{CodewalkError, Result};
use crate::{scan_files, CodeWalker, FileContent, FileEntry, FileSource, WalkConfig};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;

#[derive(Clone)]
struct StaticWalkSource {
    entries: Vec<Result<FileEntry>>,
}

impl FileSource for StaticWalkSource {
    fn walk(&self) -> Vec<Result<FileEntry>> {
        self.entries.clone()
    }
}

struct LazyOnlySource {
    entries: Vec<Result<FileEntry>>,
}

impl FileSource for LazyOnlySource {
    fn walk_lazy(&self) -> Box<dyn Iterator<Item = Result<FileEntry>> + '_> {
        Box::new(self.entries.clone().into_iter())
    }
}

fn mk_entry(path: PathBuf, is_binary: bool) -> FileEntry {
    let size = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    FileEntry {
        path,
        size,
        is_binary,
    }
}

#[test]
fn walk_config_new_matches_default() {
    let a = WalkConfig::new();
    let b = WalkConfig::default();
    assert_eq!(a.max_file_size, b.max_file_size);
    assert_eq!(a.skip_binary, b.skip_binary);
    assert_eq!(a.skip_hidden, b.skip_hidden);
}

#[test]
fn walk_config_builder_chain_sets_fields() {
    let include: HashSet<String> = ["rs".to_string(), "txt".to_string()].into_iter().collect();
    let exclude: HashSet<String> = ["bin".to_string()].into_iter().collect();
    let dirs: HashSet<String> = ["tmp".to_string(), "node_modules".to_string()]
        .into_iter()
        .collect();

    let cfg = WalkConfig::builder()
        .max_file_size(123)
        .skip_binary(false)
        .skip_hidden(false)
        .respect_gitignore(false)
        .follow_symlinks(true)
        .include_extensions(include.clone())
        .exclude_extensions(exclude.clone())
        .exclude_dirs(dirs.clone())
        .max_symlink_depth(7);

    assert_eq!(cfg.max_file_size, 123);
    assert!(!cfg.skip_binary);
    assert!(!cfg.skip_hidden);
    assert!(!cfg.respect_gitignore);
    assert!(cfg.follow_symlinks);
    assert_eq!(cfg.include_extensions, include);
    assert_eq!(cfg.exclude_extensions, exclude);
    assert_eq!(cfg.exclude_dirs, dirs);
    assert_eq!(cfg.max_symlink_depth, 7);
}

#[test]
fn walk_config_from_toml_partial_uses_defaults() {
    let cfg = WalkConfig::from_toml("skip_hidden = false").unwrap();
    assert!(!cfg.skip_hidden);
    assert!(cfg.skip_binary);
}

#[test]
fn walk_config_from_toml_invalid_returns_error() {
    let err = WalkConfig::from_toml("max_file_size = \"not-a-number\"").unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("invalid"));
}

#[test]
fn walk_config_load_reads_toml_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("walk.toml");
    fs::write(&path, "max_file_size = 2049\nskip_binary = false\n").unwrap();
    let cfg = WalkConfig::load(&path).unwrap();
    assert_eq!(cfg.max_file_size, 2049);
    assert!(!cfg.skip_binary);
}

#[test]
fn walk_config_load_missing_file_errors() {
    let missing = Path::new("/definitely/missing/codewalk-config.toml");
    let err = WalkConfig::load(missing).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("No such file") || msg.contains("not found"));
}

#[test]
fn codewalker_new_and_walk_collects_basic_files() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("a.rs"), "fn a() {}").unwrap();
    fs::write(dir.path().join("b.txt"), "hello").unwrap();

    let walker = CodeWalker::new(dir.path(), WalkConfig::default());
    let entries = walker.walk().unwrap();
    assert_eq!(entries.len(), 2);
}

#[test]
fn codewalker_into_iter_and_walk_iter_match_counts() {
    let dir = tempfile::tempdir().unwrap();
    for i in 0..5 {
        fs::write(dir.path().join(format!("{i}.rs")), "fn x(){}").unwrap();
    }

    let cfg = WalkConfig::default();
    let a = CodeWalker::new(dir.path(), cfg.clone()).walk_iter().count();
    let b = CodeWalker::new(dir.path(), cfg).into_iter().count();
    assert_eq!(a, b);
}

#[test]
fn codewalker_count_on_missing_root_is_zero() {
    let walker = CodeWalker::new("/missing/path/for/codewalk", WalkConfig::default());
    assert_eq!(walker.count(), 0);
}

#[test]
fn walk_parallel_threads_zero_is_handled_gracefully() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("one.txt"), "1").unwrap();
    let walker = CodeWalker::new(dir.path(), WalkConfig::default());
    let rx = walker.walk_parallel(0);
    let entries = rx.iter().collect::<Result<Vec<_>>>();
    assert!(entries.is_ok());
}

#[test]
fn file_entry_content_supports_unicode() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("unicode.txt");
    fs::write(&p, "hello, नमस्ते, こんにちは, 👋").unwrap();

    let entry = mk_entry(p, false);
    assert!(entry.content().unwrap().len() > 10);
    assert!(entry.content_str().unwrap().contains("नमस्ते"));
}

#[test]
fn file_entry_content_str_fails_for_non_utf8_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("bad.txt");
    fs::write(&p, vec![0xff, 0xfe, 0xfd]).unwrap();
    let entry = mk_entry(p, false);
    match entry.content_str().unwrap_err() {
        CodewalkError::Io(err) => assert_eq!(err.kind(), std::io::ErrorKind::InvalidData),
        other => panic!("unexpected error variant: {other:?}"),
    }
}

#[test]
fn file_entry_content_huge_input_roundtrips() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("huge.txt");
    let content = "x".repeat(2 * 1024 * 1024);
    fs::write(&p, &content).unwrap();
    let entry = mk_entry(p, false);
    let bytes = entry.content().unwrap();
    assert_eq!(bytes.len(), content.len());
}

#[test]
fn file_content_owned_helpers_work() {
    let fc = FileContent::Owned(vec![1, 2, 3]);
    assert_eq!(fc.as_bytes(), &[1, 2, 3]);
    assert_eq!(fc.len(), 3);
    assert!(!fc.is_empty());
    assert_eq!(fc.to_string(), "owned");
    let via_ref: &[u8] = fc.as_ref();
    assert_eq!(via_ref, &[1, 2, 3]);
}

#[test]
fn file_content_empty_helpers_work() {
    let fc = FileContent::Owned(Vec::new());
    assert_eq!(fc.len(), 0);
    assert!(fc.is_empty());
}

#[test]
fn scan_files_reads_text_and_preserves_null_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("nulls.txt");
    fs::write(&p, b"a\0b\0c").unwrap();
    let out = scan_files(dir.path()).collect::<Result<Vec<_>>>().unwrap();
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].1.as_bytes(), b"a\0b\0c");
}

#[test]
fn scan_files_empty_directory_returns_empty() {
    let dir = tempfile::tempdir().unwrap();
    let out = scan_files(dir.path()).collect::<Result<Vec<_>>>().unwrap();
    assert!(out.is_empty());
}

#[test]
fn scan_files_skips_binary_by_default() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("good.txt"), "ok").unwrap();
    fs::write(dir.path().join("bad.bin"), b"\x7fELF\x00\x00\x00\x00").unwrap();
    let out = scan_files(dir.path()).collect::<Result<Vec<_>>>().unwrap();
    assert_eq!(out.len(), 1);
    assert!(out[0].0.ends_with("good.txt"));
}

#[test]
fn file_source_default_walk_lazy_uses_walk() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("a.txt");
    fs::write(&p, "x").unwrap();
    let src = StaticWalkSource {
        entries: vec![Ok(mk_entry(p, false))],
    };
    let got = src.walk_lazy().collect::<Vec<_>>();
    assert_eq!(got.len(), 1);
}

#[test]
fn file_source_default_walk_uses_walk_lazy() {
    let src = LazyOnlySource {
        entries: vec![Err(CodewalkError::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            "boom",
        )))],
    };
    let got = src.walk();
    assert_eq!(got.len(), 1);
    assert!(got[0].is_err());
}

#[test]
fn file_source_count_counts_only_ok_entries() {
    let src = LazyOnlySource {
        entries: vec![
            Err(CodewalkError::Io(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "denied",
            ))),
            Err(CodewalkError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "missing",
            ))),
        ],
    };
    assert_eq!(src.count(), 0);
}

#[test]
fn file_source_count_with_mixed_results() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("ok.txt");
    fs::write(&p, "ok").unwrap();
    let src = StaticWalkSource {
        entries: vec![
            Ok(mk_entry(p, false)),
            Err(CodewalkError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "missing",
            ))),
        ],
    };
    assert_eq!(src.count(), 1);
}

#[test]
fn public_is_binary_errors_for_missing_file() {
    let missing = Path::new("/missing/never/exists.abc");
    let err = crate::is_binary(missing).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
}

#[test]
fn public_is_binary_detects_null_byte_content() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("suspicious.dat");
    fs::write(&p, vec![0, 1, 2, 3, 4, 5, 6, 7]).unwrap();
    assert!(crate::is_binary(&p).unwrap());
}

#[test]
fn codewalker_handles_huge_file_when_unlimited_size() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("large.log");
    fs::write(&p, "a".repeat(3 * 1024 * 1024)).unwrap();
    let cfg = WalkConfig::default().max_file_size(0).skip_binary(false);
    let entries = CodeWalker::new(dir.path(), cfg).walk().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].size, 3 * 1024 * 1024);
}

#[test]
fn concurrent_content_reads_are_consistent() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("shared.txt");
    let payload = "concurrent-read-payload".repeat(10_000);
    fs::write(&p, &payload).unwrap();
    let entry = Arc::new(mk_entry(p, false));

    let mut handles = Vec::new();
    for _ in 0..8 {
        let e = Arc::clone(&entry);
        handles.push(thread::spawn(move || e.content().unwrap().len()));
    }

    for h in handles {
        assert_eq!(h.join().unwrap(), payload.len());
    }
}

#[test]
fn concurrent_parallel_walks_on_same_tree_are_stable() {
    let dir = tempfile::tempdir().unwrap();
    for i in 0..120 {
        fs::write(dir.path().join(format!("f{i}.txt")), "x").unwrap();
    }

    let root = dir.path().to_path_buf();
    let mut handles = Vec::new();
    for _ in 0..4 {
        let root = root.clone();
        handles.push(thread::spawn(move || {
            let rx = CodeWalker::new(root, WalkConfig::default()).walk_parallel(2);
            rx.iter().collect::<Result<Vec<_>>>().unwrap().len()
        }));
    }
    for h in handles {
        assert_eq!(h.join().unwrap(), 120);
    }
}

