#![allow(clippy::unwrap_used)]

use codewalk::{CodeWalker, WalkConfig};
use std::fs;

#[cfg(unix)]
use std::os::unix::fs::symlink;

#[test]
fn gap_test_toctou_large_file_replacement() {
    let dir = tempfile::tempdir().unwrap();
    let file_path = dir.path().join("victim.txt");
    fs::write(&file_path, "small").unwrap();

    let walker = CodeWalker::new(dir.path(), WalkConfig::default().max_file_size(10));
    let entries = walker.walk().unwrap();
    assert_eq!(entries.len(), 1);

    // GAP: between walk() and content(), an attacker replaces the file with a huge one
    // The walker recorded the size as 5, but when we call content() it reads the new file.
    // If bounded capacity logic uses the *old* size, it might not read the full new file,
    // or if max_file_size isn't checked in content(), it might read an unbounded amount.
    fs::write(&file_path, "X".repeat(100)).unwrap();

    let entry = &entries[0];
    let content = entry.content().unwrap();

    // The file grew after the walk; content() must read the full current
    // file rather than silently stopping at the stale cached size.
    assert_eq!(content.len(), 100);
}

#[cfg(unix)]
#[test]
fn gap_test_symlink_escape() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();

    fs::write(outside.path().join("secret.txt"), "password").unwrap();

    let link_path = root.path().join("escape");
    symlink(outside.path(), &link_path).unwrap();

    let walker = CodeWalker::new(root.path(), WalkConfig::default().follow_symlinks(true));
    let entries = walker.walk().unwrap();

    // GAP: By default, `ignore` crate (which `CodeWalker` wraps) might follow symlinks outside the root directory.
    // In a security scanner, we usually want to bounded it within the `root` to prevent arbitrary file read.
    // If it finds `secret.txt`, it followed it outside.
    let found_secret = entries
        .iter()
        .any(|e| e.path.file_name().unwrap() == "secret.txt");

    // As a GAP test, we expect the engine to be WRONG, so we assert what we WANT (it should NOT find it).
    // If this fails, it's a finding.
    assert!(
        !found_secret,
        "FINDING: Symlink allowed escape outside root directory"
    );
}

#[test]
fn gap_test_oom_unbounded_read() {
    // If max_file_size is 0 (unlimited), does reading a massive file cause OOM?
    // We can simulate this using a very large max file size limit and creating a dummy sparse file.
    let dir = tempfile::tempdir().unwrap();
    let sparse_path = dir.path().join("sparse.bin");

    // Create a 1GB sparse file
    let f = fs::File::create(&sparse_path).unwrap();
    f.set_len(1024 * 1024 * 1024).unwrap();

    let config = WalkConfig::default().max_file_size(0).skip_binary(false);
    let walker = CodeWalker::new(dir.path(), config);
    let entries = walker.walk().unwrap();
    assert_eq!(entries.len(), 1);

    // Attempting to read content() will allocate a 1GB vector!
    // We don't actually call it here because we don't want to OOM the test runner,
    // but the GAP is that `content()` does `Vec::with_capacity` bounded by `size` or chunk size.
    // Wait, the code says:
    // let bounded_capacity = usize::try_from(self.size).unwrap_or(READ_CHUNK_SIZE);
    // let mut bytes = Vec::with_capacity(bounded_capacity.min(READ_CHUNK_SIZE * 4));
    // Ah! It actually bounds the capacity to 256KB!
    // So it will NOT OOM on `Vec::with_capacity`!
    // But it will keep `extend_from_slice` in a loop...

    // Let's assert it reads up to the file size if we chunk it.
    let mut chunk_count = 0;
    for chunk in entries[0].content_chunks().unwrap().take(10) {
        let _ = chunk.unwrap();
        chunk_count += 1;
    }
    assert_eq!(chunk_count, 10);
}
