#![allow(clippy::unwrap_used, clippy::expect_used)]

use codewalk::{CodeWalker, WalkConfig};
use std::fs;

#[test]
fn test_io_deleted_file_mid_read() {
    let dir = tempfile::tempdir().unwrap();
    let file_path = dir.path().join("victim.txt");
    fs::write(&file_path, "initial content").unwrap();

    let walker = CodeWalker::new(dir.path(), WalkConfig::default());
    let entries = walker.walk().unwrap();
    assert_eq!(entries.len(), 1, "Expected exactly 1 entry for the walk");

    // Delete the file before we read its content
    fs::remove_file(&file_path).unwrap();

    let entry = &entries[0];
    let result = entry.content();
    assert!(
        result.is_err(),
        "Expected content() to fail when file is deleted"
    );

    let err = result.unwrap_err();
    let is_not_found = matches!(err, codewalk::error::CodewalkError::Io(ref e) if e.kind() == std::io::ErrorKind::NotFound);
    assert!(is_not_found, "Expected NotFound IO error, got: {:?}", err);
}

#[cfg(unix)]
#[test]
fn test_io_permission_denied_mid_read() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let file_path = dir.path().join("secret.txt");
    fs::write(&file_path, "secret content").unwrap();

    let walker = CodeWalker::new(dir.path(), WalkConfig::default());
    let entries = walker.walk().unwrap();
    assert_eq!(entries.len(), 1, "Expected exactly 1 entry");

    // Remove read permissions
    let mut perms = fs::metadata(&file_path).unwrap().permissions();
    perms.set_mode(0o000);
    fs::set_permissions(&file_path, perms).unwrap();

    let entry = &entries[0];
    let result = entry.content();
    assert!(
        result.is_err(),
        "Expected content() to fail when permission is denied"
    );

    let err = result.unwrap_err();
    let is_permission_denied = matches!(err, codewalk::error::CodewalkError::Io(ref e) if e.kind() == std::io::ErrorKind::PermissionDenied);
    assert!(
        is_permission_denied,
        "Expected PermissionDenied IO error, got: {:?}",
        err
    );
}

#[test]
fn test_io_partial_write_toctou() {
    let dir = tempfile::tempdir().unwrap();
    let file_path = dir.path().join("partial.txt");

    // Create a 10KB file
    let initial_data = vec![b'A'; 10 * 1024];
    fs::write(&file_path, &initial_data).unwrap();

    let walker = CodeWalker::new(dir.path(), WalkConfig::default());
    let entries = walker.walk().unwrap();
    assert_eq!(entries.len(), 1, "Expected exactly 1 entry");

    let entry = &entries[0];
    assert_eq!(entry.size, 10 * 1024, "Size should be exactly 10KB");

    // Truncate the file to 5KB before reading
    let f = fs::OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(&file_path)
        .unwrap();
    f.set_len(5 * 1024).unwrap();
    drop(f);

    let content = entry
        .content()
        .expect("Should be able to read the truncated file");
    assert_eq!(
        content.len(),
        5 * 1024,
        "Expected content length to match the new truncated length, not the original size"
    );
}
