//! # codewalk
//!
//! Fast, security-aware file tree walker for codebase scanning.
//!
//! Every security tool that scans codebases needs the same thing: walk a
//! directory tree, skip binaries and huge files, respect `.gitignore`, and
//! read file contents efficiently. This crate does all of that.
//!
//! # Usage
//!
//! ```rust,no_run
//! use codewalk::{CodeWalker, WalkConfig};
//!
//! let config = WalkConfig::default();
//! let walker = CodeWalker::new("/path/to/repo", config);
//!
//! for entry in walker.walk().unwrap() {
//!     println!(
//!         "{} ({} bytes, binary={})",
//!         entry.path.display(),
//!         entry.size,
//!         entry.is_binary
//!     );
//!     if let Ok(content) = entry.content() {
//!         // scan the content...
//!         let _ = content.len();
//!     }
//! }
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

#[cfg(test)]
mod adversarial_tests;
mod detect;
/// Error types returned by codewalk APIs.
pub mod error;
mod walker;

pub use detect::is_binary;
pub use walker::{CodeWalker, FileEntry, WalkConfig};

/// Trait for anything that walks files and yields entries.
///
/// Implement this for custom file sources (S3, git objects, archives).
pub trait FileSource {
    /// Walk lazily and yield file entries on demand.
    fn walk_lazy(&self) -> Box<dyn Iterator<Item = crate::error::Result<FileEntry>> + '_> {
        Box::new(self.walk().into_iter())
    }

    /// Walk and return all file entries.
    fn walk(&self) -> Vec<crate::error::Result<FileEntry>> {
        self.walk_lazy().collect()
    }
    /// Number of files (may require a full walk).
    ///
    /// The default implementation is intentionally simple and potentially
    /// wasteful: it materializes the full walk through [`FileSource::walk`]
    /// before counting. Override this if your source can count more cheaply.
    ///
    /// Example:
    /// ```rust
    /// use codewalk::FileSource;
    ///
    /// struct StaticSource;
    ///
    /// impl FileSource for StaticSource {
    ///     fn walk(&self) -> Vec<codewalk::error::Result<codewalk::FileEntry>> {
    ///         Vec::new()
    ///     }
    /// }
    ///
    /// assert_eq!(StaticSource.count(), 0);
    /// ```
    fn count(&self) -> usize {
        self.walk().into_iter().filter(Result::is_ok).count()
    }
}

impl FileSource for CodeWalker {
    fn walk_lazy(&self) -> Box<dyn Iterator<Item = crate::error::Result<FileEntry>> + '_> {
        Box::new(self.walk_iter())
    }
}

/// Convenience: walk and read all text files, returning `(path, content)` pairs.
/// Propagates walk, I/O, and UTF-8 errors instead of silently dropping files.
///
/// Example:
/// ```rust
/// use codewalk::scan_files;
///
/// let dir = tempfile::tempdir().unwrap();
/// std::fs::write(dir.path().join("lib.rs"), "fn main() {}").unwrap();
/// let files: Vec<_> = scan_files(dir.path()).collect();
/// assert_eq!(files.len(), 1);
/// assert_eq!(files[0].as_ref().unwrap().1, "fn main() {}");
/// ```
pub fn scan_files(
    root: impl Into<std::path::PathBuf>,
) -> impl Iterator<Item = crate::error::Result<(std::path::PathBuf, String)>> {
    let walker = CodeWalker::new(root, WalkConfig::default());
    walker.into_iter().map(|entry_result| {
        let entry = entry_result?;
        let content = entry.content_str()?;
        Ok((entry.path, content))
    })
}
