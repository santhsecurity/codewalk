//! File tree walker — the core iteration engine.

use std::collections::HashSet;
use std::io;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc};

use ignore::WalkBuilder;

use crate::detect;

const DEFAULT_MMAP_THRESHOLD: u64 = 64 * 1024;
const DEFAULT_MAX_SYMLINK_DEPTH: usize = 16;
const READ_CHUNK_SIZE: usize = 64 * 1024;

/// Configuration for directory walking.
///
/// Loadable from TOML:
/// ```toml
/// max_file_size = 10485760  # 10 MB
/// skip_binary = true
/// skip_hidden = true
/// respect_gitignore = true
/// follow_symlinks = false
/// include_extensions = ["rs", "py", "js"]
/// exclude_dirs = ["node_modules", ".git", "target"]
/// ```
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct WalkConfig {
    /// Maximum file size in bytes to include (0 = unlimited).
    pub max_file_size: u64,
    /// Skip files detected as binary.
    pub skip_binary: bool,
    /// Skip hidden files and directories (dotfiles).
    pub skip_hidden: bool,
    /// Respect `.gitignore` rules.
    pub respect_gitignore: bool,
    /// Follow symbolic links while walking.
    pub follow_symlinks: bool,
    /// Only include files with these extensions (empty = all).
    pub include_extensions: HashSet<String>,
    /// Exclude files with these extensions.
    pub exclude_extensions: HashSet<String>,
    /// Directories to always skip.
    pub exclude_dirs: HashSet<String>,
    /// Use memory-mapped I/O for file reading (faster for large files).
    pub use_mmap: bool,
    /// Minimum file size for mmap (below this, use regular read).
    pub mmap_threshold: u64,
    /// Maximum number of symlink hops allowed in any discovered path.
    pub max_symlink_depth: usize,
}

impl Default for WalkConfig {
    fn default() -> Self {
        let exclude_dirs: HashSet<String> = [
            "node_modules",
            ".git",
            "target",
            "__pycache__",
            ".venv",
            "venv",
            ".tox",
            ".mypy_cache",
            ".pytest_cache",
            "dist",
            "build",
            ".next",
            ".nuxt",
            "vendor",
            ".bundle",
            ".gradle",
            ".mvn",
            "Pods",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();

        Self {
            max_file_size: 10 * 1024 * 1024, // 10 MB
            skip_binary: true,
            skip_hidden: true,
            respect_gitignore: true,
            follow_symlinks: false,
            include_extensions: HashSet::new(),
            exclude_extensions: HashSet::new(),
            exclude_dirs,
            use_mmap: true,
            mmap_threshold: DEFAULT_MMAP_THRESHOLD,
            max_symlink_depth: DEFAULT_MAX_SYMLINK_DEPTH,
        }
    }
}

impl WalkConfig {
    /// Create a new default configuration.
    ///
    /// Example:
    /// ```rust
    /// use codewalk::WalkConfig;
    ///
    /// let config = WalkConfig::new();
    /// assert!(config.skip_binary);
    /// ```
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a fluent builder starting from sensible defaults.
    ///
    /// Example:
    /// ```rust
    /// use codewalk::WalkConfig;
    ///
    /// let config = WalkConfig::builder().max_file_size(1024).skip_hidden(false);
    /// assert_eq!(config.max_file_size, 1024);
    /// ```
    #[must_use]
    pub fn builder() -> Self {
        Self::default()
    }

    /// Load configuration from a TOML file.
    ///
    /// Missing fields use defaults. Unknown fields are ignored.
    ///
    /// Example:
    /// ```rust
    /// use codewalk::WalkConfig;
    ///
    /// let dir = tempfile::tempdir().unwrap();
    /// let path = dir.path().join("walk.toml");
    /// std::fs::write(&path, "max_file_size = 2048\n").unwrap();
    /// let config = WalkConfig::load(&path).unwrap();
    /// assert_eq!(config.max_file_size, 2048);
    /// ```
    pub fn load(path: &std::path::Path) -> Result<Self, Box<dyn std::error::Error>> {
        let content = std::fs::read_to_string(path)?;
        let config: WalkConfig = toml::from_str(&content)?;
        Ok(config)
    }

    /// Parse configuration from a TOML string.
    ///
    /// Example:
    /// ```rust
    /// use codewalk::WalkConfig;
    ///
    /// let config = WalkConfig::from_toml("skip_hidden = false").unwrap();
    /// assert!(!config.skip_hidden);
    /// ```
    pub fn from_toml(toml_str: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(toml_str)
    }

    /// Set maximum file size in bytes.
    #[must_use]
    pub fn max_file_size(mut self, max_file_size: u64) -> Self {
        self.max_file_size = max_file_size;
        self
    }

    /// Configure whether binary files are skipped.
    #[must_use]
    pub fn skip_binary(mut self, skip_binary: bool) -> Self {
        self.skip_binary = skip_binary;
        self
    }

    /// Configure whether hidden files are skipped.
    #[must_use]
    pub fn skip_hidden(mut self, skip_hidden: bool) -> Self {
        self.skip_hidden = skip_hidden;
        self
    }

    /// Configure `.gitignore` handling.
    #[must_use]
    pub fn respect_gitignore(mut self, respect_gitignore: bool) -> Self {
        self.respect_gitignore = respect_gitignore;
        self
    }

    /// Configure symbolic link traversal.
    #[must_use]
    pub fn follow_symlinks(mut self, follow_symlinks: bool) -> Self {
        self.follow_symlinks = follow_symlinks;
        self
    }

    /// Set extensions to include.
    #[must_use]
    pub fn include_extensions(mut self, include_extensions: HashSet<String>) -> Self {
        self.include_extensions = include_extensions;
        self
    }

    /// Set extensions to exclude.
    #[must_use]
    pub fn exclude_extensions(mut self, exclude_extensions: HashSet<String>) -> Self {
        self.exclude_extensions = exclude_extensions;
        self
    }

    /// Set directories to always skip.
    #[must_use]
    pub fn exclude_dirs(mut self, exclude_dirs: HashSet<String>) -> Self {
        self.exclude_dirs = exclude_dirs;
        self
    }

    /// Configure memory-mapped file reading.
    #[must_use]
    pub fn use_mmap(mut self, use_mmap: bool) -> Self {
        self.use_mmap = use_mmap;
        self
    }

    /// Set mmap threshold in bytes.
    #[must_use]
    pub fn mmap_threshold(mut self, mmap_threshold: u64) -> Self {
        self.mmap_threshold = mmap_threshold;
        self
    }

    /// Set the maximum number of symlink hops to follow per path.
    #[must_use]
    pub fn max_symlink_depth(mut self, max_symlink_depth: usize) -> Self {
        self.max_symlink_depth = max_symlink_depth;
        self
    }
}

/// A discovered file entry with lazy content loading.
#[derive(Clone, Debug)]
pub struct FileEntry {
    /// Absolute path to the file.
    pub path: PathBuf,
    /// File size in bytes.
    pub size: u64,
    /// Whether the file was detected as binary.
    pub is_binary: bool,
}

impl FileEntry {
    /// Read the file content.
    ///
    /// Reads files using bounded buffered I/O. Content is NOT cached —
    /// each call reads from disk.
    ///
    /// Example:
    /// ```rust
    /// use codewalk::CodeWalker;
    ///
    /// let dir = tempfile::tempdir().unwrap();
    /// std::fs::write(dir.path().join("sample.txt"), "hello").unwrap();
    /// let entry = CodeWalker::new(dir.path(), Default::default()).walk().unwrap().remove(0);
    /// assert_eq!(entry.content().unwrap().as_bytes(), b"hello");
    /// ```
    pub fn content(&self) -> crate::error::Result<FileContent> {
        let mut file = std::fs::File::open(&self.path)?;
        let bounded_capacity = usize::try_from(self.size).unwrap_or(READ_CHUNK_SIZE);
        let mut bytes = Vec::with_capacity(bounded_capacity.min(READ_CHUNK_SIZE * 4));
        let mut chunk = [0_u8; READ_CHUNK_SIZE];

        loop {
            let read = file.read(&mut chunk)?;
            if read == 0 {
                break;
            }
            bytes.extend_from_slice(&chunk[..read]);
        }

        Ok(FileContent::Owned(bytes))
    }

    /// Read the file content as a UTF-8 string.
    ///
    /// Example:
    /// ```rust
    /// use codewalk::CodeWalker;
    ///
    /// let dir = tempfile::tempdir().unwrap();
    /// std::fs::write(dir.path().join("sample.txt"), "hello").unwrap();
    /// let entry = CodeWalker::new(dir.path(), Default::default()).walk().unwrap().remove(0);
    /// assert_eq!(entry.content_str().unwrap(), "hello");
    /// ```
    pub fn content_str(&self) -> crate::error::Result<String> {
        std::fs::read_to_string(&self.path)
    }
}

/// File content — either memory-mapped or heap-allocated.
#[derive(Debug)]
pub enum FileContent {
    /// Heap-allocated bytes (for small files).
    Owned(Vec<u8>),
}

impl std::fmt::Display for FileContent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("owned")
    }
}

impl FileContent {
    /// Get the content as a byte slice.
    ///
    /// Example:
    /// ```rust
    /// use codewalk::CodeWalker;
    ///
    /// let dir = tempfile::tempdir().unwrap();
    /// std::fs::write(dir.path().join("sample.txt"), "abc").unwrap();
    /// let entry = CodeWalker::new(dir.path(), Default::default()).walk().unwrap().remove(0);
    /// assert_eq!(entry.content().unwrap().as_bytes(), b"abc");
    /// ```
    pub fn as_bytes(&self) -> &[u8] {
        match self {
            Self::Owned(v) => v.as_slice(),
        }
    }

    /// Content length.
    ///
    /// Example:
    /// ```rust
    /// use codewalk::CodeWalker;
    ///
    /// let dir = tempfile::tempdir().unwrap();
    /// std::fs::write(dir.path().join("sample.txt"), "abc").unwrap();
    /// let entry = CodeWalker::new(dir.path(), Default::default()).walk().unwrap().remove(0);
    /// assert_eq!(entry.content().unwrap().len(), 3);
    /// ```
    pub fn len(&self) -> usize {
        self.as_bytes().len()
    }

    /// Whether the content is empty.
    ///
    /// Example:
    /// ```rust
    /// use codewalk::CodeWalker;
    ///
    /// let dir = tempfile::tempdir().unwrap();
    /// std::fs::write(dir.path().join("sample.txt"), "").unwrap();
    /// let entry = CodeWalker::new(dir.path(), Default::default()).walk().unwrap().remove(0);
    /// assert!(entry.content().unwrap().is_empty());
    /// ```
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl AsRef<[u8]> for FileContent {
    fn as_ref(&self) -> &[u8] {
        self.as_bytes()
    }
}

/// File tree walker for codebase scanning.
pub struct CodeWalker {
    root: PathBuf,
    config: WalkConfig,
}

impl CodeWalker {
    /// Create a new walker rooted at the given path.
    ///
    /// Example:
    /// ```rust
    /// use codewalk::{CodeWalker, WalkConfig};
    ///
    /// let walker = CodeWalker::new(".", WalkConfig::default());
    /// let _ = walker;
    /// ```
    pub fn new(root: impl Into<PathBuf>, config: WalkConfig) -> Self {
        Self {
            root: root.into(),
            config,
        }
    }

    /// Walk the tree, yielding file entries.
    ///
    /// Example:
    /// ```rust
    /// use codewalk::{CodeWalker, WalkConfig};
    ///
    /// let dir = tempfile::tempdir().unwrap();
    /// std::fs::write(dir.path().join("main.rs"), "fn main() {}").unwrap();
    /// let files = CodeWalker::new(dir.path(), WalkConfig::default()).walk().unwrap();
    /// assert_eq!(files.len(), 1);
    /// ```
    pub fn walk(&self) -> crate::error::Result<Vec<FileEntry>> {
        self.walk_iter().collect()
    }

    /// Walk the tree as an iterator.
    ///
    /// Example:
    /// ```rust
    /// use codewalk::{CodeWalker, WalkConfig};
    ///
    /// let dir = tempfile::tempdir().unwrap();
    /// std::fs::write(dir.path().join("main.rs"), "fn main() {}").unwrap();
    /// let count = CodeWalker::new(dir.path(), WalkConfig::default()).walk_iter().count();
    /// assert_eq!(count, 1);
    /// ```
    pub fn walk_iter(&self) -> impl Iterator<Item = crate::error::Result<FileEntry>> + '_ {
        let config = Arc::new(self.config.clone());
        self.build_walker().filter_map(move |result| match result {
            Ok(entry) => match entry.file_type() {
                Some(ft) if ft.is_file() => match process_path(entry.path(), config.as_ref()) {
                    Ok(Some(file_entry)) => Some(Ok(file_entry)),
                    Ok(None) => None,
                    Err(err) => Some(Err(err)),
                },
                _ => None,
            },
            Err(err) => Some(Err(crate::error::CodewalkError::Ignore(err))),
        })
    }

    /// Walk the tree in parallel, sending entries to a channel.
    ///
    /// Returns a receiver. The walker runs on `threads` background threads.
    /// Entries arrive as they're discovered — no need to wait for the full walk.
    ///
    /// Example:
    /// ```rust
    /// use codewalk::{CodeWalker, WalkConfig};
    ///
    /// let dir = tempfile::tempdir().unwrap();
    /// std::fs::write(dir.path().join("main.rs"), "fn main() {}").unwrap();
    /// let rx = CodeWalker::new(dir.path(), WalkConfig::default()).walk_parallel(1);
    /// assert!(rx.recv().unwrap().is_ok());
    /// ```
    pub fn walk_parallel(&self, threads: usize) -> mpsc::Receiver<crate::error::Result<FileEntry>> {
        // Use a bounded channel to enforce backpressure. If the consumer is slower than the disk reader,
        // an unbounded channel will buffer millions of entries in memory and cause an OOM crash.
        let (tx, rx) = mpsc::sync_channel(8192);
        let config = Arc::new(self.config.clone());
        let walker = self.build_parallel_walker(threads, Arc::clone(&config));

        std::thread::spawn(move || {
            walker.run(|| {
                let tx = tx.clone();
                let config = Arc::clone(&config);
                Box::new(move |result| {
                    let entry_result = match result {
                        Ok(entry) => match entry.file_type() {
                            Some(ft) if ft.is_file() => {
                                match process_path(entry.path(), config.as_ref()) {
                                    Ok(Some(file_entry)) => Ok(file_entry),
                                    Ok(None) => return ignore::WalkState::Continue,
                                    Err(err) => Err(err),
                                }
                            }
                            _ => return ignore::WalkState::Continue,
                        },
                        Err(err) => Err(crate::error::CodewalkError::Ignore(err)),
                    };

                    if tx.send(entry_result).is_err() {
                        return ignore::WalkState::Quit;
                    }

                    ignore::WalkState::Continue
                })
            });
        });

        rx
    }

    /// Total number of files (requires full walk — use for progress bars).
    pub fn count(&self) -> usize {
        self.walk().map(|entries| entries.len()).unwrap_or_default()
    }

    fn build_walker(&self) -> ignore::Walk {
        WalkBuilder::new(&self.root)
            .hidden(self.config.skip_hidden)
            .git_ignore(self.config.respect_gitignore)
            .git_global(self.config.respect_gitignore)
            .git_exclude(self.config.respect_gitignore)
            .follow_links(self.config.follow_symlinks)
            .filter_entry({
                let exclude_dirs = self.config.exclude_dirs.clone();
                let config = self.config.clone();
                move |entry| {
                    if entry.file_type().is_some_and(|ft| ft.is_dir()) {
                        let name = entry.file_name().to_string_lossy();
                        if exclude_dirs.contains(name.as_ref()) {
                            return false;
                        }
                    }
                    entry_allowed(entry.path(), &config)
                }
            })
            .build()
    }

    fn build_parallel_walker(
        &self,
        threads: usize,
        config: Arc<WalkConfig>,
    ) -> ignore::WalkParallel {
        WalkBuilder::new(&self.root)
            .hidden(self.config.skip_hidden)
            .git_ignore(self.config.respect_gitignore)
            .git_global(self.config.respect_gitignore)
            .git_exclude(self.config.respect_gitignore)
            .follow_links(self.config.follow_symlinks)
            .threads(threads)
            .filter_entry({
                let exclude_dirs = self.config.exclude_dirs.clone();
                move |entry| {
                    if entry.file_type().is_some_and(|ft| ft.is_dir()) {
                        let name = entry.file_name().to_string_lossy();
                        if exclude_dirs.contains(name.as_ref()) {
                            return false;
                        }
                    }
                    entry_allowed(entry.path(), config.as_ref())
                }
            })
            .build_parallel()
    }
}

impl IntoIterator for CodeWalker {
    type Item = crate::error::Result<FileEntry>;
    type IntoIter = Box<dyn Iterator<Item = crate::error::Result<FileEntry>>>;

    fn into_iter(self) -> Self::IntoIter {
        let config = Arc::new(self.config.clone());
        Box::new(
            WalkBuilder::new(&self.root)
                .hidden(self.config.skip_hidden)
                .git_ignore(self.config.respect_gitignore)
                .git_global(self.config.respect_gitignore)
                .git_exclude(self.config.respect_gitignore)
                .follow_links(self.config.follow_symlinks)
                .filter_entry({
                    let exclude_dirs = self.config.exclude_dirs.clone();
                    let config = self.config.clone();
                    move |entry| {
                        if entry.file_type().is_some_and(|ft| ft.is_dir()) {
                            let name = entry.file_name().to_string_lossy();
                            if exclude_dirs.contains(name.as_ref()) {
                                return false;
                            }
                        }
                        entry_allowed(entry.path(), &config)
                    }
                })
                .build()
                .filter_map(move |result| match result {
                    Ok(entry) => match entry.file_type() {
                        Some(ft) if ft.is_file() => {
                            match process_path(entry.path(), config.as_ref()) {
                                Ok(Some(file_entry)) => Some(Ok(file_entry)),
                                Ok(None) => None,
                                Err(err) => Some(Err(err)),
                            }
                        }
                        _ => None,
                    },
                    Err(err) => Some(Err(crate::error::CodewalkError::Ignore(err))),
                }),
        )
    }
}

fn entry_allowed(path: &Path, config: &WalkConfig) -> bool {
    if !config.follow_symlinks {
        return true;
    }

    let depth = symlink_depth(path).unwrap_or(usize::MAX);
    depth <= config.max_symlink_depth && !has_symlink_loop(path)
}

fn process_path(path: &Path, config: &WalkConfig) -> crate::error::Result<Option<FileEntry>> {
    let metadata = std::fs::metadata(path)?;
    let size = metadata.len();

    // Size filter.
    if config.max_file_size > 0 && size > config.max_file_size {
        return Ok(None);
    }

    // Extension filter.
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        let lower = ext.to_ascii_lowercase();
        if !config.include_extensions.is_empty() && !config.include_extensions.contains(&lower) {
            return Ok(None);
        }
        if config.exclude_extensions.contains(&lower) {
            return Ok(None);
        }
    } else if !config.include_extensions.is_empty() {
        // No extension and we have an include filter — skip.
        return Ok(None);
    }

    // Binary detection.
    let is_bin = if size == 0 {
        false
    } else {
        detect::is_binary(path)?
    };
    if config.skip_binary && is_bin {
        return Ok(None);
    }

    Ok(Some(FileEntry {
        path: path.to_path_buf(),
        size,
        is_binary: is_bin,
    }))
}

fn symlink_depth(path: &Path) -> crate::error::Result<usize> {
    let mut depth = 0usize;
    let mut current = PathBuf::new();

    for component in path.components() {
        current.push(component);
        let metadata = match std::fs::symlink_metadata(&current) {
            Ok(metadata) => metadata,
            Err(_) => continue,
        };
        if metadata.file_type().is_symlink() {
            depth = depth.saturating_add(1);
        }
    }

    Ok(depth)
}

fn has_symlink_loop(path: &Path) -> bool {
    let mut seen = HashSet::new();
    let mut current = PathBuf::new();

    for component in path.components() {
        current.push(component);
        let Ok(metadata) = std::fs::symlink_metadata(&current) else {
            continue;
        };
        if !metadata.file_type().is_symlink() {
            continue;
        }

        let Ok(canonical) = std::fs::canonicalize(&current) else {
            continue;
        };
        if !seen.insert(canonical) {
            return true;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[cfg(unix)]
    use std::os::unix::ffi::OsStringExt;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    #[cfg(unix)]
    fn symlink_dir(src: &Path, dst: &Path) -> crate::error::Result<()> {
        std::os::unix::fs::symlink(src, dst)
    }

    #[cfg(windows)]
    fn symlink_dir(src: &Path, dst: &Path) -> crate::error::Result<()> {
        std::os::windows::fs::symlink_dir(src, dst)
    }

    fn symlink_enabled_config() -> WalkConfig {
        WalkConfig {
            follow_symlinks: true,
            ..WalkConfig::default()
        }
    }

    fn setup_test_dir() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("main.rs"), "fn main() {}").unwrap();
        fs::write(dir.path().join("lib.rs"), "pub fn hello() {}").unwrap();
        fs::write(dir.path().join("data.bin"), b"\x7fELF\x00\x00\x00\x00").unwrap();
        fs::create_dir(dir.path().join("node_modules")).unwrap();
        fs::write(dir.path().join("node_modules/junk.js"), "// junk").unwrap();
        fs::create_dir(dir.path().join("src")).unwrap();
        fs::write(dir.path().join("src/app.py"), "print('hello')").unwrap();
        dir
    }

    #[test]
    fn walks_directory() {
        let dir = setup_test_dir();
        let walker = CodeWalker::new(dir.path(), WalkConfig::default());
        let entries = walker.walk().unwrap();
        // Should find main.rs, lib.rs, src/app.py (not data.bin, not node_modules/)
        assert!(entries.len() >= 2);
        let paths: Vec<String> = entries
            .iter()
            .map(|e| e.path.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        assert!(paths.contains(&"main.rs".to_string()));
        assert!(paths.contains(&"lib.rs".to_string()));
        assert!(!paths.contains(&"data.bin".to_string())); // binary skipped
        assert!(!paths.contains(&"junk.js".to_string())); // node_modules skipped
    }

    #[test]
    fn respects_include_extensions() {
        let dir = setup_test_dir();
        let config = WalkConfig {
            include_extensions: ["rs"].iter().map(|s| s.to_string()).collect(),
            ..WalkConfig::default()
        };
        let walker = CodeWalker::new(dir.path(), config);
        let entries = walker.walk().unwrap();
        assert!(entries.iter().all(|e| e.path.extension().unwrap() == "rs"));
    }

    #[test]
    fn respects_exclude_extensions() {
        let dir = setup_test_dir();
        let config = WalkConfig {
            exclude_extensions: ["py"].iter().map(|s| s.to_string()).collect(),
            ..WalkConfig::default()
        };
        let walker = CodeWalker::new(dir.path(), config);
        let entries = walker.walk().unwrap();
        assert!(entries.iter().all(|e| e.path.extension().unwrap() != "py"));
    }

    #[test]
    fn respects_max_file_size() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("small.txt"), "hi").unwrap();
        fs::write(dir.path().join("big.txt"), "x".repeat(1000)).unwrap();

        let config = WalkConfig {
            max_file_size: 100,
            skip_binary: false,
            ..WalkConfig::default()
        };
        let walker = CodeWalker::new(dir.path(), config);
        let entries = walker.walk().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path.file_name().unwrap(), "small.txt");
    }

    #[test]
    fn includes_binary_when_not_skipped() {
        let dir = setup_test_dir();
        let config = WalkConfig {
            skip_binary: false,
            ..WalkConfig::default()
        };
        let walker = CodeWalker::new(dir.path(), config);
        let entries = walker.walk().unwrap();
        let has_bin = entries
            .iter()
            .any(|e| e.path.file_name().unwrap() == "data.bin");
        assert!(has_bin);
    }

    #[test]
    fn file_content_read() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("test.txt"), "hello world").unwrap();

        let config = WalkConfig {
            skip_binary: false,
            ..WalkConfig::default()
        };
        let walker = CodeWalker::new(dir.path(), config);
        let entries = walker.walk().unwrap();
        assert_eq!(entries.len(), 1);

        let content = entries[0].content().unwrap();
        assert_eq!(content.as_bytes(), b"hello world");
        assert_eq!(content.len(), 11);
        assert!(!content.is_empty());
    }

    #[test]
    fn file_content_str() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("test.rs"), "fn main() {}").unwrap();

        let walker = CodeWalker::new(dir.path(), WalkConfig::default());
        let entries = walker.walk().unwrap();
        let s = entries[0].content_str().unwrap();
        assert_eq!(s, "fn main() {}");
    }

    #[test]
    fn parallel_walk() {
        let dir = setup_test_dir();
        let walker = CodeWalker::new(dir.path(), WalkConfig::default());
        let rx = walker.walk_parallel(2);
        let entries: Vec<FileEntry> = rx.iter().collect::<Result<Vec<_>, _>>().unwrap();
        assert!(entries.len() >= 2);
    }

    #[test]
    fn empty_directory() {
        let dir = tempfile::tempdir().unwrap();
        let walker = CodeWalker::new(dir.path(), WalkConfig::default());
        let entries = walker.walk().unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn count_matches_walk() {
        let dir = setup_test_dir();
        let walker = CodeWalker::new(dir.path(), WalkConfig::default());
        let count = walker.count();
        let entries = walker.walk().unwrap();
        assert_eq!(count, entries.len());
    }

    #[test]
    fn default_config_excludes_common_dirs() {
        let config = WalkConfig::default();
        assert!(config.exclude_dirs.contains("node_modules"));
        assert!(config.exclude_dirs.contains(".git"));
        assert!(config.exclude_dirs.contains("target"));
        assert!(config.exclude_dirs.contains("__pycache__"));
        assert!(config.exclude_dirs.contains("vendor"));
    }

    #[test]
    fn walk_iter_collects_entries() {
        let dir = setup_test_dir();
        let walker = CodeWalker::new(dir.path(), WalkConfig::default());
        let entries: Vec<FileEntry> = walker.walk_iter().collect::<Result<Vec<_>, _>>().unwrap();
        let paths: Vec<&Path> = entries.iter().map(|entry| entry.path.as_path()).collect();
        assert!(paths.iter().any(|p| p.ends_with("main.rs")));
        assert!(paths.iter().any(|p| p.ends_with("lib.rs")));
        assert!(paths.iter().any(|p| p.ends_with("src/app.py")));
    }

    #[test]
    fn follows_symlinks_when_enabled() {
        let dir = tempfile::tempdir().unwrap();
        let real_dir = dir.path().join("real");
        fs::create_dir(&real_dir).unwrap();
        fs::write(real_dir.join("inside.txt"), "linked").unwrap();

        let linked_dir = dir.path().join("linked");
        symlink_dir(&real_dir, &linked_dir).unwrap();

        let linked_inside = linked_dir.join("inside.txt");

        let entries = CodeWalker::new(dir.path(), WalkConfig::default())
            .walk()
            .unwrap();
        assert!(!entries.iter().any(|entry| entry.path == linked_inside));

        let entries = CodeWalker::new(dir.path(), symlink_enabled_config())
            .walk()
            .unwrap();
        assert!(entries.iter().any(|entry| entry.path == linked_inside));
    }

    #[cfg(unix)]
    #[test]
    fn handles_non_utf8_filenames() {
        use std::ffi::OsString;

        let dir = tempfile::tempdir().unwrap();
        let invalid_name = {
            let mut raw = b"bad-".to_vec();
            raw.extend_from_slice(b"\xffname.txt");
            OsString::from_vec(raw)
        };
        let path = dir.path().join(&invalid_name);
        fs::write(&path, "unicode").unwrap();

        let walker = CodeWalker::new(dir.path(), WalkConfig::default());
        let entries = walker.walk().unwrap();
        assert!(entries.iter().any(|entry| entry.path == path));
    }

    #[test]
    fn handles_empty_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty.txt");
        fs::write(&path, b"").unwrap();

        let walker = CodeWalker::new(dir.path(), WalkConfig::default());
        let entries = walker.walk().unwrap();
        let entry = entries.iter().find(|entry| entry.path == path);
        assert!(entry.is_some());
        let entry = entry.unwrap();
        assert_eq!(entry.size, 0);
        assert!(!entry.is_binary);
    }

    #[cfg(unix)]
    #[test]
    fn handles_permission_denied() {
        let dir = tempfile::tempdir().unwrap();
        let public_file = dir.path().join("public.txt");
        fs::write(&public_file, "allowed").unwrap();

        let blocked_dir = dir.path().join("blocked");
        fs::create_dir(&blocked_dir).unwrap();
        let blocked_file = blocked_dir.join("secret.txt");
        fs::write(&blocked_file, "secret").unwrap();

        let original_permissions = fs::metadata(&blocked_dir).unwrap().permissions();
        let mut blocked_permissions = original_permissions.clone();
        blocked_permissions.set_mode(0o000);
        fs::set_permissions(&blocked_dir, blocked_permissions).unwrap();

        let can_read_blocked_dir = fs::read_dir(&blocked_dir).is_ok();

        let results: Vec<_> = CodeWalker::new(dir.path(), WalkConfig::default())
            .walk_iter()
            .collect();
        let _ = fs::set_permissions(&blocked_dir, original_permissions);

        let entries: Vec<_> = results
            .iter()
            .filter_map(|result| result.as_ref().ok())
            .collect();
        assert!(entries.iter().any(|entry| entry.path == public_file));
        if !can_read_blocked_dir {
            assert!(!entries
                .iter()
                .any(|entry| entry.path.starts_with(&blocked_dir)));
        }
    }
}
