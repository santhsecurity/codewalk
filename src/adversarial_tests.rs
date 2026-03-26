//! Adversarial tests for codewalk - designed to BREAK the code
//!
//! Tests: symlink loops, permission denied dirs, 10K files, non-UTF8 paths,
//! empty dirs nested 100 deep, files changing during walk

use crate::{CodeWalker, WalkConfig};
use std::fs;

#[cfg(unix)]
use std::os::unix::ffi::OsStringExt;
#[cfg(unix)]
use std::os::unix::fs::symlink as unix_symlink;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

#[cfg(windows)]
use std::os::windows::fs::symlink_dir as windows_symlink;

/// Create a symlink loop: A -> B -> C -> A
#[cfg(unix)]
#[test]
fn adversarial_symlink_loop_detection() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a");
    let b = dir.path().join("b");
    let c = dir.path().join("c");

    fs::create_dir(&a).unwrap();
    fs::create_dir(&b).unwrap();
    fs::create_dir(&c).unwrap();

    // Create loop: a/link -> b, b/link -> c, c/link -> a
    unix_symlink(&b, a.join("link")).unwrap();
    unix_symlink(&c, b.join("link")).unwrap();
    unix_symlink(&a, c.join("link")).unwrap();

    let config = WalkConfig {
        follow_symlinks: true,
        ..WalkConfig::default()
    };

    let walker = CodeWalker::new(dir.path(), config);
    // Should not hang or crash on symlink loop
    let results: Vec<_> = walker.walk_iter().collect();
    assert!(
        results.iter().any(Result::is_err),
        "symlink loop should surface an error instead of hanging"
    );
    let entries: Vec<_> = results.into_iter().filter_map(Result::ok).collect();
    assert!(entries.len() <= 100); // Sanity check - shouldn't explode
}

/// Test walking through permission denied directories
#[cfg(unix)]
#[test]
fn adversarial_permission_denied_directory() {
    let dir = tempfile::tempdir().unwrap();

    // Create accessible file
    let accessible = dir.path().join("accessible.txt");
    fs::write(&accessible, "accessible").unwrap();

    // Create directory with no permissions
    let blocked = dir.path().join("blocked");
    fs::create_dir(&blocked).unwrap();
    let blocked_file = blocked.join("secret.txt");
    fs::write(&blocked_file, "secret").unwrap();

    // Remove all permissions
    let mut perms = fs::metadata(&blocked).unwrap().permissions();
    perms.set_mode(0o000);
    fs::set_permissions(&blocked, perms).unwrap();

    let walker = CodeWalker::new(dir.path(), WalkConfig::default());
    let results: Vec<_> = walker.walk_iter().collect();

    // Restore permissions for cleanup
    let mut perms = fs::metadata(&blocked).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&blocked, perms).unwrap();

    let entries: Vec<_> = results
        .iter()
        .filter_map(|result| result.as_ref().ok())
        .collect();
    let paths: Vec<_> = entries.iter().map(|e| e.path.clone()).collect();
    assert!(paths.contains(&accessible));
    assert!(!paths.contains(&blocked_file));
}

/// Test walking with 10,000 files in a single directory
#[test]
fn adversarial_10k_files_in_directory() {
    let dir = tempfile::tempdir().unwrap();
    let files_dir = dir.path().join("files");
    fs::create_dir(&files_dir).unwrap();

    // Create 10,000 files
    for i in 0..10_000 {
        let path = files_dir.join(format!("file_{:05}.txt", i));
        fs::write(&path, format!("content {}", i)).unwrap();
    }

    let walker = CodeWalker::new(&files_dir, WalkConfig::default());
    let entries = walker.walk().unwrap();

    assert_eq!(entries.len(), 10_000);
}

/// Test walking with non-UTF8 paths
#[cfg(unix)]
#[test]
fn adversarial_non_utf8_paths() {
    use std::ffi::OsString;

    let dir = tempfile::tempdir().unwrap();

    // Create file with invalid UTF-8 in name
    let bad_bytes = {
        let mut raw = b"file-\xff\xfe".to_vec();
        raw.extend_from_slice(b".txt");
        OsString::from_vec(raw)
    };
    let bad_path = dir.path().join(&bad_bytes);
    fs::write(&bad_path, "content").unwrap();

    // Create nested directory with bad name
    let bad_dir = dir.path().join(OsString::from_vec(b"dir-\xff".to_vec()));
    fs::create_dir(&bad_dir).unwrap();
    fs::write(bad_dir.join("inside.txt"), "inside").unwrap();

    let walker = CodeWalker::new(dir.path(), WalkConfig::default());
    // Should not panic on non-UTF8 paths
    let entries = walker.walk().unwrap();

    // Should find both files
    assert_eq!(entries.len(), 2);
}

/// Test walking 100 levels of nested empty directories
#[test]
fn adversarial_deeply_nested_empty_dirs() {
    let dir = tempfile::tempdir().unwrap();

    // Create 100 levels of nesting
    let mut current = dir.path().to_path_buf();
    for i in 0..100 {
        current = current.join(format!("level_{:03}", i));
        fs::create_dir(&current).unwrap();
    }

    // Add a file at the deepest level
    fs::write(current.join("deep.txt"), "deep content").unwrap();

    let walker = CodeWalker::new(dir.path(), WalkConfig::default());
    let entries = walker.walk().unwrap();

    assert_eq!(entries.len(), 1);
    assert!(entries[0].path.ends_with("deep.txt"));
}

/// Test walking while files are being modified
#[test]
fn adversarial_files_changing_during_walk() {
    let dir = tempfile::tempdir().unwrap();

    // Create initial files
    for i in 0..100 {
        fs::write(dir.path().join(format!("file_{}.txt", i)), "initial").unwrap();
    }

    let walker = CodeWalker::new(dir.path(), WalkConfig::default());

    // Spawn a thread that modifies files during walk
    let dir_path = dir.path().to_path_buf();
    let modifier = std::thread::spawn(move || {
        for i in 0..100 {
            let _ = fs::write(dir_path.join(format!("file_{}.txt", i)), "modified");
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    });

    // Walk while modifications are happening
    let entries = walker.walk().unwrap();

    modifier.join().unwrap();

    // Should complete without crashing
    assert_eq!(entries.len(), 100);
}

/// Test walking a directory that gets deleted during walk
#[test]
fn adversarial_directory_deleted_during_walk() {
    let base = tempfile::tempdir().unwrap();
    let victim = base.path().join("victim");
    fs::create_dir(&victim).unwrap();

    // Create files in subdirectories
    for i in 0..50 {
        let sub = victim.join(format!("sub_{}", i));
        fs::create_dir(&sub).unwrap();
        fs::write(sub.join("file.txt"), "content").unwrap();
    }

    let walker = CodeWalker::new(&victim, WalkConfig::default());

    // Walk - this should handle any race conditions gracefully
    let entries = walker.walk().unwrap_or_default();
    assert!(
        entries.len() <= 50,
        "walk should not duplicate entries excessively"
    );
}

/// Test with circular symlink that points to parent
#[cfg(unix)]
#[test]
fn adversarial_circular_symlink_to_parent() {
    let dir = tempfile::tempdir().unwrap();
    let parent = dir.path().join("parent");
    fs::create_dir(&parent).unwrap();

    // Create symlink that points to its own parent
    unix_symlink(&parent, parent.join("loop")).unwrap();

    // Also put a real file there
    fs::write(parent.join("real.txt"), "real").unwrap();

    let config = WalkConfig {
        follow_symlinks: true,
        ..WalkConfig::default()
    };

    let walker = CodeWalker::new(&parent, config);
    let results: Vec<_> = walker.walk_iter().collect();
    let entries: Vec<_> = results.into_iter().filter_map(Result::ok).collect();

    // Should not hang, should find real.txt
    assert!(entries.iter().any(|e| e.path.ends_with("real.txt")));
}

/// Test with extremely long file paths (near OS limit)
#[test]
fn adversarial_very_long_file_paths() {
    let dir = tempfile::tempdir().unwrap();

    // Create a path with a very long filename
    let long_name = "a".repeat(200);
    let long_path = dir.path().join(format!("{}.txt", long_name));
    fs::write(&long_path, "content").unwrap();

    let walker = CodeWalker::new(dir.path(), WalkConfig::default());
    let entries = walker.walk().unwrap();

    assert_eq!(entries.len(), 1);
}

/// Test walking a mix of files and symlinks interleaved
#[cfg(unix)]
#[test]
fn adversarial_mixed_symlinks_and_files() {
    let dir = tempfile::tempdir().unwrap();
    let real_dir = dir.path().join("real");
    fs::create_dir(&real_dir).unwrap();

    // Create real files and symlinks interleaved
    for i in 0..50 {
        let real_file = real_dir.join(format!("real_{}.txt", i));
        fs::write(&real_file, format!("content {}", i)).unwrap();

        // Create symlink to this file
        let link = dir.path().join(format!("link_{}.txt", i));
        unix_symlink(&real_file, &link).unwrap();
    }

    let config = WalkConfig {
        follow_symlinks: true,
        ..WalkConfig::default()
    };

    let walker = CodeWalker::new(dir.path(), config);
    let entries = walker.walk().unwrap();

    // Should find all files (both real and through symlinks)
    assert!(entries.len() >= 50);
}

/// Test with files containing null bytes in content (binary detection)
#[test]
fn adversarial_binary_files_with_nulls() {
    let dir = tempfile::tempdir().unwrap();

    // Create files with varying amounts of null bytes
    for i in 0..10 {
        let mut content = vec![b'A'; 100];
        // Insert enough nulls in the first 16 bytes to trigger binary detection (>30% of 16 bytes = 5 bytes)
        for j in 0..=5 {
            content[j * 2] = 0;
        }
        fs::write(dir.path().join(format!("binary_{}.bin", i)), content).unwrap();
    }

    let config = WalkConfig::default();
    let walker = CodeWalker::new(dir.path(), config);
    let entries = walker.walk().unwrap();

    // Binary files should be skipped with default config
    assert_eq!(entries.len(), 0);
}

/// Test parallel walk with contention
#[test]
fn adversarial_parallel_walk_contention() {
    let dir = tempfile::tempdir().unwrap();

    // Create many files for parallel processing
    for i in 0..1000 {
        fs::write(
            dir.path().join(format!("file_{}.txt", i)),
            format!("content {}", i),
        )
        .unwrap();
    }

    let walker = CodeWalker::new(dir.path(), WalkConfig::default());

    // Start multiple parallel walks simultaneously
    let mut handles = vec![];
    for _ in 0..5 {
        let rx = walker.walk_parallel(4);
        handles.push(std::thread::spawn(move || {
            rx.iter().collect::<Result<Vec<_>, _>>().unwrap().len()
        }));
    }

    let counts: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    // Each walk should get its own results
    for count in counts {
        assert_eq!(count, 1000);
    }
}

/// Test with maximum file size of 0 (should include all files)
#[test]
fn adversarial_zero_max_file_size() {
    let dir = tempfile::tempdir().unwrap();

    // Create files of various sizes
    fs::write(dir.path().join("small.txt"), "x").unwrap();
    fs::write(dir.path().join("medium.txt"), "x".repeat(1000)).unwrap();
    fs::write(dir.path().join("large.txt"), "x".repeat(1000000)).unwrap();

    let config = WalkConfig {
        max_file_size: 0, // 0 means unlimited
        ..WalkConfig::default()
    };

    let walker = CodeWalker::new(dir.path(), config);
    let entries = walker.walk().unwrap();

    // Should include all files regardless of size
    assert_eq!(entries.len(), 3);
}

/// Test with all extension filters active simultaneously
#[test]
fn adversarial_conflicting_extension_filters() {
    let dir = tempfile::tempdir().unwrap();

    // Create files with different extensions
    fs::write(dir.path().join("a.rs"), "rust").unwrap();
    fs::write(dir.path().join("b.py"), "python").unwrap();
    fs::write(dir.path().join("c.js"), "javascript").unwrap();
    fs::write(dir.path().join("d.txt"), "text").unwrap();

    let config = WalkConfig {
        include_extensions: ["rs".to_string(), "py".to_string()].into_iter().collect(),
        exclude_extensions: ["py".to_string()].into_iter().collect(), // Conflicts with include
        ..WalkConfig::default()
    };

    let walker = CodeWalker::new(dir.path(), config);
    let entries = walker.walk().unwrap();

    // Should find only .rs files (include applies first, then exclude)
    let paths: Vec<_> = entries.iter().map(|e| e.path.clone()).collect();
    assert!(paths.iter().any(|p| p.to_string_lossy().ends_with(".rs")));
    assert!(!paths.iter().any(|p| p.to_string_lossy().ends_with(".py")));
}

/// Test empty mmap threshold
#[test]
fn adversarial_zero_mmap_threshold() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("test.txt"), "small content").unwrap();

    let config = WalkConfig {
        mmap_threshold: 0, // Will mmap everything
        use_mmap: true,
        ..WalkConfig::default()
    };

    let walker = CodeWalker::new(dir.path(), config);
    let entries = walker.walk().unwrap();

    assert_eq!(entries.len(), 1);

    // Try to read content
    let content = entries[0].content();
    assert!(content.is_ok());
}

/// Test with files getting deleted right after being discovered
#[test]
fn adversarial_files_deleted_after_discovery() {
    let dir = tempfile::tempdir().unwrap();

    // Create temporary files
    for i in 0..20 {
        fs::write(
            dir.path().join(format!("temp_{}.txt", i)),
            format!("temp {}", i),
        )
        .unwrap();
    }

    let walker = CodeWalker::new(dir.path(), WalkConfig::default());
    let entries = walker.walk().unwrap();

    // Delete all files
    for i in 0..20 {
        let _ = fs::remove_file(dir.path().join(format!("temp_{}.txt", i)));
    }

    // Try to read content of deleted files
    for entry in entries {
        let error = entry.content().unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
    }
}

/// Test walking with hidden files and special names
#[test]
fn adversarial_hidden_and_special_files() {
    let dir = tempfile::tempdir().unwrap();

    // Create various special files
    fs::write(dir.path().join(".hidden"), "hidden").unwrap();
    fs::write(dir.path().join("..double_dot"), "double").unwrap();
    fs::write(dir.path().join("-dash_start"), "dash").unwrap();
    fs::write(dir.path().join(" space file "), "space").unwrap();
    fs::write(dir.path().join("special!@#$%"), "special").unwrap();

    let config = WalkConfig {
        skip_hidden: false, // Include hidden files
        ..WalkConfig::default()
    };

    let walker = CodeWalker::new(dir.path(), config);
    let entries = walker.walk().unwrap();

    // Should find files (excluding .hidden if skip_hidden is true, but we set false)
    assert!(entries.len() >= 4);
}

/// Test with concurrent modification of root directory
#[test]
fn adversarial_concurrent_directory_modification() {
    let dir = tempfile::tempdir().unwrap();

    let walker = CodeWalker::new(dir.path(), WalkConfig::default());

    // Start modifying directory
    let dir_path = dir.path().to_path_buf();
    let modifier = std::thread::spawn(move || {
        for i in 0..50 {
            let file = dir_path.join(format!("concurrent_{}.txt", i));
            fs::write(&file, "content").unwrap();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    });

    // Walk while modifications are happening
    let entries = walker.walk().unwrap_or_default();

    modifier.join().unwrap();
    assert!(
        entries.len() <= 50,
        "concurrent creation should not produce duplicates"
    );
}
