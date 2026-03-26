//! Binary file detection via magic bytes.

use std::fs::File;
use std::io::Read;
use std::path::Path;

/// Known binary file magic bytes.
const MAGIC_BYTES: &[(&[u8], &str)] = &[
    (b"\x7fELF", "ELF"),
    (b"MZ", "PE/COFF"),
    (b"\xfe\xed\xfa\xce", "Mach-O 32"),
    (b"\xfe\xed\xfa\xcf", "Mach-O 64"),
    (b"\xce\xfa\xed\xfe", "Mach-O 32 (reversed)"),
    (b"\xcf\xfa\xed\xfe", "Mach-O 64 (reversed)"),
    (b"\xca\xfe\xba\xbe", "Java class / Mach-O fat"),
    (b"\x00asm", "WASM"),
    (b"PK\x03\x04", "ZIP/JAR/APK"),
    (b"\x1f\x8b", "gzip"),
    (b"BZ", "bzip2"),
    (b"\xfd7zXZ", "xz"),
    (b"\x89PNG", "PNG"),
    (b"\xff\xd8\xff", "JPEG"),
    (b"GIF8", "GIF"),
    (b"RIFF", "RIFF (WAV/AVI/WebP)"),
    (b"\x00\x00\x01\x00", "ICO"),
    (b"SQLite format 3", "SQLite"),
    (b"\x04\x22\x4d\x18", "LZ4"),
    (b"\x28\xb5\x2f\xfd", "Zstandard"),
];

/// Known binary file extensions (case-insensitive).
const BINARY_EXTENSIONS: &[&str] = &[
    "exe", "dll", "so", "dylib", "a", "lib", "o", "obj", "class", "jar", "war", "ear", "wasm",
    "zip", "tar", "gz", "bz2", "xz", "zst", "7z", "rar", "png", "jpg", "jpeg", "gif", "bmp", "ico",
    "webp", "svg", "mp3", "mp4", "avi", "mkv", "mov", "flac", "wav", "ogg", "pdf", "doc", "docx",
    "xls", "xlsx", "ppt", "pptx", "ttf", "otf", "woff", "woff2", "eot", "db", "sqlite", "sqlite3",
    "pyc", "pyo", "min.js", "min.css",
];

/// Check if a file is binary using magic bytes and extension heuristics.
///
/// Reads the first 16 bytes and checks against known binary signatures.
/// Falls back to extension checking if magic bytes don't match.
pub fn is_binary(path: &Path) -> std::io::Result<bool> {
    // Check extension first (cheap).
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        let lower = ext.to_ascii_lowercase();
        if BINARY_EXTENSIONS.contains(&lower.as_str()) {
            return Ok(true);
        }
        // Check compound extensions like .min.js
        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
            if stem.ends_with(".min") && (lower == "js" || lower == "css") {
                return Ok(true);
            }
        }
    }

    // Check magic bytes.
    let mut file = File::open(path)?;
    let mut buf = [0u8; 16];
    let n = file.read(&mut buf)?;
    if n == 0 {
        return Ok(false);
    }

    for (magic, _name) in MAGIC_BYTES {
        if n >= magic.len() && buf[..magic.len()] == **magic {
            return Ok(true);
        }
    }

    // Heuristic: if >30% of first bytes are non-text, it's binary.
    let non_text = buf[..n]
        .iter()
        .filter(|&&b| b == 0 || (b < 7) || (b > 14 && b < 32 && b != 27))
        .count();

    Ok((non_text as f64 / n as f64) > 0.30)
}

#[cfg(test)]
mod tests {
    use super::*;
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
}
