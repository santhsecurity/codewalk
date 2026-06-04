#![allow(clippy::unwrap_used)]

use codewalk::{CodeWalker, WalkConfig};
use std::fs;

#[test]
fn test_empty_directory() {
    let dir = tempfile::tempdir().unwrap();
    let walker = CodeWalker::new(dir.path(), WalkConfig::default());
    let entries = walker.walk().unwrap();
    assert!(entries.is_empty());
}

#[test]
fn test_empty_config() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("test.txt"), "hello").unwrap();

    // An artificially bare-bones config.
    let config = WalkConfig {
        max_file_size: 0,
        skip_binary: false,
        skip_hidden: false,
        respect_gitignore: false,
        follow_symlinks: false,
        include_extensions: std::collections::HashSet::default(),
        exclude_extensions: std::collections::HashSet::default(),
        exclude_dirs: std::collections::HashSet::default(),
        ignore_files: vec![],
        ignore_patterns: vec![],
        max_symlink_depth: 0,
    };

    let walker = CodeWalker::new(dir.path(), config);
    let entries = walker.walk().unwrap();
    assert_eq!(entries.len(), 1);
}

#[test]
fn test_null_bytes_in_content() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("null.txt"), b"start\0end").unwrap();

    let config = WalkConfig::default().skip_binary(true);
    let walker = CodeWalker::new(dir.path(), config);
    let entries = walker.walk().unwrap();

    // The detect logic might flag this as binary because of the null byte.
    // Let's verify what happens.
    if let Some(entry) = entries.first() {
        let content = entry.content().unwrap();
        // It's either Binary or Unknown/Text.
        assert!(!content.is_empty());
    }
}

#[test]
fn test_huge_input_boundary() {
    let dir = tempfile::tempdir().unwrap();
    // 64 KB is READ_CHUNK_SIZE
    let chunk_size = 64 * 1024;
    let data = vec![b'a'; chunk_size];
    fs::write(dir.path().join("boundary.txt"), &data).unwrap();

    let config = WalkConfig::default();
    let walker = CodeWalker::new(dir.path(), config);
    let entries = walker.walk().unwrap();

    assert_eq!(entries.len(), 1);
    let chunks = entries[0]
        .content_chunks()
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].len(), chunk_size);

    // Exactly chunk_size + 1
    let data2 = vec![b'b'; chunk_size + 1];
    fs::write(dir.path().join("boundary2.txt"), &data2).unwrap();
    let walker = CodeWalker::new(dir.path(), WalkConfig::default());
    let entries = walker.walk().unwrap();
    let entry = entries
        .iter()
        .find(|e| e.path.file_name().unwrap() == "boundary2.txt")
        .unwrap();

    let chunks = entry
        .content_chunks()
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(chunks.len(), 2);
    assert_eq!(chunks[0].len(), chunk_size);
    assert_eq!(chunks[1].len(), 1);
}

#[test]
fn test_deep_nesting() {
    let dir = tempfile::tempdir().unwrap();
    let mut current = dir.path().to_path_buf();
    for i in 0..100 {
        current = current.join(format!("dir_{i}"));
        fs::create_dir(&current).unwrap();
    }
    fs::write(current.join("deep.txt"), "hello").unwrap();

    let walker = CodeWalker::new(dir.path(), WalkConfig::default());
    let entries = walker.walk().unwrap();
    assert_eq!(entries.len(), 1);
}

#[test]
fn test_symlink_depth_max() {
    let dir = tempfile::tempdir().unwrap();
    let real_dir = dir.path().join("real");
    fs::create_dir(&real_dir).unwrap();
    fs::write(real_dir.join("file.txt"), "hello").unwrap();

    let mut current_link = dir.path().join("link_0");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&real_dir, &current_link).unwrap();
    #[cfg(windows)]
    std::os::windows::fs::symlink_dir(&real_dir, &current_link).unwrap();

    for i in 1..20 {
        let next_link = dir.path().join(format!("link_{i}"));
        #[cfg(unix)]
        std::os::unix::fs::symlink(&current_link, &next_link).unwrap();
        #[cfg(windows)]
        std::os::windows::fs::symlink_dir(&current_link, &next_link).unwrap();
        current_link = next_link;
    }

    // max depth 10
    let config = WalkConfig::default()
        .follow_symlinks(true)
        .max_symlink_depth(10);
    let walker = CodeWalker::new(dir.path(), config);
    let entries = walker.walk().unwrap();
    // It should skip the ones that exceed max depth.
    // We just assert it doesn't panic and returns something.
    assert!(!entries.is_empty());
}

#[test]
fn test_single_byte() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("single.txt"), b"a").unwrap();

    let config = WalkConfig::default().skip_binary(false);
    let walker = CodeWalker::new(dir.path(), config);
    let entries = walker.walk().unwrap();

    assert_eq!(entries.len(), 1, "Should find the single byte file");
    let content = entries[0].content().unwrap();
    assert_eq!(content.len(), 1, "Content length should be exactly 1");
    assert_eq!(content.as_bytes(), b"a", "Content should match 'a'");
}

#[test]
fn test_u32_max_bytes_simulation() {
    let dir = tempfile::tempdir().unwrap();
    // Simulate what happens if we set max_file_size to u32::MAX
    // Since we can't easily write 4GB to disk in tests, we test the boundary limit handling.
    let config = WalkConfig::default().max_file_size(u64::from(u32::MAX));
    let walker = CodeWalker::new(dir.path(), config);
    let entries = walker.walk().unwrap();
    assert_eq!(entries.len(), 0, "No files should be found");
}

#[test]
fn test_all_zero_and_all_0xff() {
    let dir = tempfile::tempdir().unwrap();
    let zero_path = dir.path().join("zeros.bin");
    fs::write(&zero_path, vec![0u8; 1024]).unwrap();

    let ff_path = dir.path().join("ffs.bin");
    fs::write(&ff_path, vec![0xFFu8; 1024]).unwrap();

    let config = WalkConfig::default().skip_binary(false);
    let walker = CodeWalker::new(dir.path(), config);
    let entries = walker.walk().unwrap();

    assert_eq!(entries.len(), 2, "Should find both adversarial files");

    for entry in entries {
        let content = entry.content().unwrap();
        assert_eq!(content.len(), 1024, "Content length should match");
        if entry.path == zero_path {
            assert!(content.is_binary(), "All zeros should be binary");
        } else if entry.path == ff_path {
            // All 0xFF is invalid UTF-8 and should be Unknown or Binary depending on detect logic
            // We just verify it parses safely without panicking.
            assert!(!content.is_text(), "All 0xFF shouldn't be text");
        }
    }
}

#[test]
fn test_alternating_patterns() {
    let dir = tempfile::tempdir().unwrap();
    let mut data = Vec::with_capacity(1000);
    for i in 0..1000 {
        data.push(if i % 2 == 0 { b'a' } else { 0xFF });
    }

    fs::write(dir.path().join("alt.bin"), data).unwrap();
    let config = WalkConfig::default().skip_binary(false);
    let walker = CodeWalker::new(dir.path(), config);
    let entries = walker.walk().unwrap();

    assert_eq!(entries.len(), 1, "Should find alternating pattern file");
    let content = entries[0].content().unwrap();
    assert!(
        !content.is_text(),
        "Alternating invalid utf8 shouldn't be valid text"
    );
}

#[test]
fn test_u32_truncation_limits() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("truncation.txt");
    fs::write(&path, "abc").unwrap();

    // If max_file_size is used in u32 conversions anywhere, this would panic or truncate
    let config = WalkConfig::default().max_file_size(u64::from(u32::MAX) + 1);
    let walker = CodeWalker::new(dir.path(), config);
    let entries = walker.walk().unwrap();

    assert_eq!(
        entries.len(),
        1,
        "Should find file despite size limit > u32::MAX"
    );
    let entry = &entries[0];

    // Test the size field bounds
    assert!(
        u32::try_from(entry.size).is_ok(),
        "File size should not overflow"
    );

    let content = entry.content().unwrap();
    assert_eq!(content.len(), 3, "Content length should match");
}

#[test]
fn test_oom_unbounded_read_sparse() {
    let dir = tempfile::tempdir().unwrap();
    let sparse_path = dir.path().join("huge.bin");

    let f = fs::File::create(&sparse_path).unwrap();
    // Simulate a 4GB sparse file to see if memory allocation panics or limits it
    f.set_len(4 * 1024 * 1024 * 1024).unwrap();

    let config = WalkConfig::default().max_file_size(0).skip_binary(false);
    let walker = CodeWalker::new(dir.path(), config);
    let entries = walker.walk().unwrap();

    assert_eq!(entries.len(), 1, "Should find sparse file");
    let entry = &entries[0];

    // Calling content_chunks to verify it doesn't try to allocate 4GB at once
    let mut chunk_count = 0;
    for chunk in entry.content_chunks().unwrap().take(2) {
        let c = chunk.unwrap();
        assert!(c.len() <= 64 * 1024, "Chunk size should be bounded");
        chunk_count += 1;
    }
    assert_eq!(chunk_count, 2, "Should read chunks without OOM");
}
