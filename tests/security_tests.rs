#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

use codewalk::{CodeWalker, WalkConfig};

#[cfg(unix)]
use std::os::unix::fs::symlink;

fn setup_test_dir() -> TempDir {
    tempfile::tempdir().expect("failed to create temp dir")
}

fn create_file<P: AsRef<Path>>(path: P, content: &[u8]) {
    let mut f = File::create(path).expect("failed to create file");
    f.write_all(content).expect("failed to write file");
}

fn create_dir<P: AsRef<Path>>(path: P) {
    fs::create_dir_all(path).expect("failed to create dir");
}

fn walk_paths(walker: CodeWalker) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for result in walker.walk_iter() {
        if let Ok(entry) = result {
            paths.push(entry.path);
        }
    }
    paths.sort();
    paths
}

#[cfg(unix)]
#[test]
fn test_01_symlink_outside_root() {
    let td = setup_test_dir();
    let root = td.path().join("root");
    let outside = td.path().join("outside");
    create_dir(&root);
    create_dir(&outside);
    create_file(outside.join("secret.txt"), b"secret");

    // Create symlink pointing outside
    symlink(&outside, root.join("link_to_outside")).unwrap();

    let config = WalkConfig {
        follow_symlinks: true,
        ..WalkConfig::default()
    };
    let walker = CodeWalker::new(&root, config);
    let paths = walk_paths(walker);

    // It should not traverse outside the root.
    // It might list the symlink itself if walkdir includes it, or skip it,
    // but definitely NOT `secret.txt`.
    assert!(!paths.iter().any(|p| p.ends_with("secret.txt")));
}

#[cfg(unix)]
#[test]
fn test_02_symlink_loop() {
    let td = setup_test_dir();
    let root = td.path();
    let dir_a = root.join("a");
    let dir_b = root.join("b");
    create_dir(&dir_a);
    create_dir(&dir_b);

    symlink(&dir_b, dir_a.join("link_to_b")).unwrap();
    symlink(&dir_a, dir_b.join("link_to_a")).unwrap();

    let config = WalkConfig {
        follow_symlinks: true,
        ..WalkConfig::default()
    };
    let walker = CodeWalker::new(root, config);
    // Should complete without infinite loop
    let _paths = walk_paths(walker);
}

#[cfg(unix)]
#[test]
fn test_03_symlink_chain_depth() {
    let td = setup_test_dir();
    let root = td.path();

    let mut last_target = root.join("target.txt");
    create_file(&last_target, b"end of chain");

    // Create a chain of 20 symlinks
    for i in 0..20 {
        let link = root.join(format!("link_{}", i));
        symlink(&last_target, &link).unwrap();
        last_target = link;
    }

    let config = WalkConfig {
        follow_symlinks: true,
        ..WalkConfig::default()
    };
    let walker = CodeWalker::new(root, config);
    // Walk shouldn't hang or crash. The walker should reject deep chains.
    let _paths = walk_paths(walker);
}

#[test]
fn test_04_directory_with_100k_files() {
    let td = setup_test_dir();
    let root = td.path();

    // Instead of actually creating 100K files which might slow down test suite,
    // we'll do 10K. It still tests memory/OOM resilience for a large directory.
    let target = 10_000;
    for i in 0..target {
        create_file(root.join(format!("file_{}.txt", i)), b"test");
    }

    let walker = CodeWalker::new(root, WalkConfig::default());
    let paths = walk_paths(walker);
    // Just checking it completes and collects all files
    assert_eq!(paths.len(), target);
}

#[test]
fn test_05_file_named_env() {
    let td = setup_test_dir();
    let root = td.path();
    create_file(root.join(".env"), b"SECRET=123");

    let walker = CodeWalker::new(root, WalkConfig::default().skip_hidden(false));
    let paths = walk_paths(walker);

    assert!(paths.iter().any(|p| p.ends_with(".env")));
}

#[test]
fn test_06_node_modules_excluded() {
    let td = setup_test_dir();
    let root = td.path();
    let nm = root.join("node_modules");
    create_dir(&nm);
    create_file(nm.join("index.js"), b"console.log('hi');");
    create_file(root.join("main.js"), b"console.log('main');");

    let walker = CodeWalker::new(root, WalkConfig::default());
    let paths = walk_paths(walker);

    assert!(
        !paths
            .iter()
            .any(|p| p.to_string_lossy().contains("node_modules"))
    );
    assert!(paths.iter().any(|p| p.ends_with("main.js")));
}

#[test]
fn test_07_git_excluded() {
    let td = setup_test_dir();
    let root = td.path();
    let git = root.join(".git");
    create_dir(&git);
    create_file(git.join("config"), b"[core]");
    create_file(root.join("main.rs"), b"fn main() {}");

    let walker = CodeWalker::new(root, WalkConfig::default());
    let paths = walk_paths(walker);

    assert!(!paths.iter().any(|p| p.to_string_lossy().contains(".git")));
    assert!(paths.iter().any(|p| p.ends_with("main.rs")));
}

#[test]
fn test_08_target_excluded() {
    let td = setup_test_dir();
    let root = td.path();
    let target = root.join("target");
    create_dir(&target);
    create_file(target.join("debug.log"), b"log");
    create_file(root.join("main.rs"), b"fn main() {}");

    let walker = CodeWalker::new(root, WalkConfig::default());
    let paths = walk_paths(walker);

    assert!(!paths.iter().any(|p| p.to_string_lossy().contains("target")));
    assert!(paths.iter().any(|p| p.ends_with("main.rs")));
}

#[test]
fn test_09_binary_file_skip_true() {
    let td = setup_test_dir();
    let root = td.path();
    // ELF magic bytes
    let elf_magic = b"\x7fELF\x02\x01\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00";
    create_file(root.join("prog.bin"), elf_magic);
    create_file(root.join("text.txt"), b"hello");

    let config = WalkConfig {
        skip_binary: true,
        ..WalkConfig::default()
    };
    let walker = CodeWalker::new(root, config);
    let paths = walk_paths(walker);

    assert!(!paths.iter().any(|p| p.ends_with("prog.bin")));
    assert!(paths.iter().any(|p| p.ends_with("text.txt")));
}

#[test]
fn test_10_binary_file_skip_false() {
    let td = setup_test_dir();
    let root = td.path();
    let elf_magic = b"\x7fELF\x02\x01\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00";
    create_file(root.join("prog.bin"), elf_magic);
    create_file(root.join("text.txt"), b"hello");

    let config = WalkConfig {
        skip_binary: false,
        ..WalkConfig::default()
    };
    let walker = CodeWalker::new(root, config);
    let paths = walk_paths(walker);

    assert!(paths.iter().any(|p| p.ends_with("prog.bin")));
    assert!(paths.iter().any(|p| p.ends_with("text.txt")));
}
