//! Binary file detection via magic bytes.

use std::collections::HashSet;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::OnceLock;

use crate::probe::file_type::BINARY_MAGIC_BYTES;

/// Known binary file extensions (case-insensitive).
///
/// Stored as a `&[&str]` source-of-truth; the `OnceLock<HashSet>` below
/// provides O(1) lookup at runtime. Both must stay in sync.
const BINARY_EXTENSIONS_RAW: &[&str] = &[
    "exe", "dll", "so", "dylib", "a", "lib", "o", "obj", "class", "jar", "war", "ear", "wasm",
    "zip", "tar", "gz", "bz2", "xz", "zst", "7z", "rar", "png", "jpg", "jpeg", "gif", "bmp",
    "ico", "webp", "mp3", "mp4", "avi", "mkv", "mov", "flac", "wav", "ogg", "pdf", "doc", "docx",
    "xls", "xlsx", "ppt", "pptx", "ttf", "otf", "woff", "woff2", "eot", "db", "sqlite",
    "sqlite3", "pyc", "pyo",
];

fn binary_extensions() -> &'static HashSet<&'static str> {
    static SET: OnceLock<HashSet<&'static str>> = OnceLock::new();
    SET.get_or_init(|| BINARY_EXTENSIONS_RAW.iter().copied().collect())
}

/// Check if a file is binary using magic bytes and extension heuristics.
///
/// Reads the first 16 bytes and checks against known binary signatures.
/// Falls back to extension checking if magic bytes don't match.
/// # Errors
/// Returns an error if the file cannot be read.
pub fn is_binary(path: &Path) -> std::io::Result<bool> {
    // Check extension first (O(1) hash lookup via the cached set).
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        let lower = ext.to_ascii_lowercase();
        if binary_extensions().contains(lower.as_str()) {
            return Ok(true);
        }
        // Check compound extensions like .min.js
        if let Some(stem) = path.file_stem().and_then(|s| s.to_str())
            && stem.to_ascii_lowercase().ends_with(".min")
            && (lower == "js" || lower == "css")
        {
            return Ok(true);
        }
    }

    // Check magic bytes.
    let mut file = File::open(path)?;
    is_binary_file(path, &mut file)
}

pub(crate) fn is_binary_file(_path: &Path, file: &mut File) -> std::io::Result<bool> {
    // Read up to 512 bytes to allow detection of formats whose signature
    // appears beyond the first 16 bytes (e.g., tar ustar at offset 257).
    file.seek(SeekFrom::Start(0))?;
    const BUF_SZ: usize = 512;
    let mut buf = vec![0u8; BUF_SZ];
    let n = file.read(&mut buf)?;
    if n == 0 {
        return Ok(false);
    }
    buf.truncate(n);

    // Check magic prefixes in the beginning of the file.
    for magic in BINARY_MAGIC_BYTES {
        if n >= magic.len() && buf[..magic.len()] == **magic {
            return Ok(true);
        }
    }

    // Check for tar ustar signature at offset 257 within the buffer.
    if n >= 257 + 6 {
        if &buf[257..257 + 6] == b"ustar\0" {
            return Ok(true);
        }
    }

    // Heuristic: if >30% of the sampled bytes are non-text, it's binary.
    let non_text = buf
        .iter()
        .filter(|&&b| b == 0 || (b < 7) || (b > 14 && b < 32 && b != 27))
        .count();

    #[allow(clippy::cast_precision_loss)]
    Ok((non_text as f64 / n as f64) > 0.30)
}
