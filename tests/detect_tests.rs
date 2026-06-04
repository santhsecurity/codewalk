#![allow(clippy::unwrap_used)]

use codewalk::detect::is_binary;
use std::path::Path;

#[test]
fn binary_extensions() {
    assert!(is_binary(Path::new("test.exe")).unwrap());
    assert!(is_binary(Path::new("lib.so")).unwrap());
    assert!(is_binary(Path::new("image.png")).unwrap());
    assert!(is_binary(Path::new("archive.zip")).unwrap());
    assert!(is_binary(Path::new("data.sqlite3")).unwrap());
}

#[test]
fn text_extensions() {
    let dir = tempfile::tempdir().unwrap();
    for name in ["main.rs", "index.js", "style.css", "README.md"] {
        let path = dir.path().join(name);
        std::fs::write(&path, "plain text").unwrap();
        assert!(
            !is_binary(&path).unwrap(),
            "{name} should be detected as text"
        );
    }
}

#[test]
fn minified_assets() {
    assert!(is_binary(Path::new("bundle.min.js")).unwrap());
    assert!(is_binary(Path::new("styles.min.css")).unwrap());
}

#[test]
fn nonexistent_file_returns_error() {
    let error = is_binary(Path::new("/nonexistent/path/xyz.abc")).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
}

#[test]
fn real_text_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.txt");
    std::fs::write(&path, "Hello, world! This is a text file.\n").unwrap();
    assert!(!is_binary(&path).unwrap());
}

#[test]
fn real_binary_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.bin");
    std::fs::write(
        &path,
        b"\x7fELF\x02\x01\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00",
    )
    .unwrap();
    assert!(is_binary(&path).unwrap());
}

#[test]
fn null_bytes_detected() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nulls.dat");
    std::fs::write(&path, b"\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00").unwrap();
    assert!(is_binary(&path).unwrap());
}

#[test]
fn empty_file_is_text() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("empty.txt");
    std::fs::write(&path, b"").unwrap();
    assert!(!is_binary(&path).unwrap());
}
