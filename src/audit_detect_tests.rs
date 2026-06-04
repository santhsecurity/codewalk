#![allow(clippy::unwrap_used)]

use crate::detect;
use crate::{CodeWalker, WalkConfig};
use std::fs;
use tempfile::tempdir;

#[test]
fn verify_tar_archive_is_detected_as_binary() {
    // Create a minimal tar-like file where the ustar signature appears at offset 257.
    let dir = tempdir().unwrap();
    let p = dir.path().join("archive.tar");

    // Build 512-byte tar header where bytes 257..263 == b"ustar\0"
    let mut block = vec![b' '; 512];
    block[257..263].copy_from_slice(b"ustar\0");
    fs::write(&p, &block).unwrap();

    // Detection via public API should treat this as binary/archive and return true.
    // The implementation currently only inspects the first 16 bytes and may return false.
    let is_bin = detect::is_binary(&p).unwrap();
    assert!(is_bin, "tar archive should be detected as binary (not text)");
}

#[test]
fn verify_root_symlink_test_helper() {
    // Reuse the audit test to exercise walking a symlink root.
    let dir = tempdir().unwrap();
    let real = dir.path().join("real");
    fs::create_dir_all(&real).unwrap();
    fs::write(real.join("file.txt"), "hi").unwrap();

    let link = dir.path().join("link");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&real, &link).unwrap();
    #[cfg(windows)]
    std::os::windows::fs::symlink_dir(&real, &link).unwrap();

    let walker = CodeWalker::new(&link, WalkConfig::default());
    let entries = walker.walk().unwrap();
    assert!(entries.iter().any(|e| e.path.file_name().unwrap() == "file.txt"));
}
