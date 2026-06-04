use std::path::PathBuf;
use std::sync::Arc;

use ignore::WalkBuilder;
use ignore::overrides::OverrideBuilder;

use super::{
    FileEntry, WalkConfig,
    filter::{entry_allowed, process_path},
};

/// File tree walker for codebase scanning.
pub struct CodeWalker {
    pub(crate) root: PathBuf,
    pub(crate) config: WalkConfig,
}

impl CodeWalker {
    /// Create a new walker rooted at the given path.
    pub fn new(root: impl Into<PathBuf>, config: WalkConfig) -> Self {
        Self {
            root: root.into(),
            config,
        }
    }

    /// Walk the tree, yielding file entries.
    ///
    /// # Errors
    /// Returns an error if directory traversal fails or a file cannot be processed.
    pub fn walk(&self) -> crate::error::Result<Vec<FileEntry>> {
        self.walk_iter().collect()
    }

    /// Walk the tree and return entries sorted by absolute path.
    ///
    /// # Errors
    /// Returns an error if directory traversal fails or a file cannot be processed.
    pub fn walk_sorted(&self) -> crate::error::Result<Vec<FileEntry>> {
        let mut entries = self.walk()?;
        entries.sort_by(|left, right| left.path.cmp(&right.path));
        Ok(entries)
    }

    /// Walk the tree as an iterator.
    pub fn walk_iter(&self) -> impl Iterator<Item = crate::error::Result<FileEntry>> + '_ {
        let config = Arc::new(self.config.clone());
        let _root = self.root.clone();
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

    /// Total number of files (requires full walk  -  use for progress bars).
    #[must_use]
    pub fn count(&self) -> usize {
        self.walk_iter().filter_map(Result::ok).count()
    }

    pub(crate) fn build_walker(&self) -> ignore::Walk {
        let mut builder = WalkBuilder::new(&self.root);
        builder
            .hidden(self.config.skip_hidden)
            .git_ignore(self.config.respect_gitignore)
            .git_global(self.config.respect_gitignore)
            .git_exclude(self.config.respect_gitignore)
            .follow_links(self.config.follow_symlinks);

        for ignore_file in &self.config.ignore_files {
            builder.add_custom_ignore_filename(ignore_file);
        }

        if !self.config.ignore_patterns.is_empty() {
            let mut ovr = OverrideBuilder::new(&self.root);
            for pattern in &self.config.ignore_patterns {
                if let Err(err) = ovr.add(pattern) {
                    tracing::warn!(pattern = %pattern, error = %err, "invalid ignore pattern");
                }
            }
            match ovr.build() {
                Ok(overrides) => {
                    builder.overrides(overrides);
                }
                Err(err) => {
                    tracing::warn!(error = %err, "failed to build ignore overrides");
                }
            }
        }

        let root = self.root.clone();
        builder
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
                    entry_allowed(entry.path(), &root, &config)
                }
            })
            .build()
    }
}

impl IntoIterator for CodeWalker {
    type Item = crate::error::Result<FileEntry>;
    type IntoIter = Box<dyn Iterator<Item = crate::error::Result<FileEntry>>>;

    /// Delegates to `build_walker` so the walker-construction logic lives in
    /// exactly one place.  The previous implementation copy-pasted the entire
    /// `WalkBuilder` setup here — any future change to `build_walker` would
    /// silently diverge from this path.
    ///
    /// `walk_iter` cannot be used directly because it borrows `&self` and
    /// returns a lifetime-bound iterator; `into_iter` consumes `self` and
    /// must return a `'static` iterator.  We therefore call `build_walker`
    /// on the consumed `self`, clone the `Arc<WalkConfig>` for the closure,
    /// and build the iterator directly — sharing the builder logic without
    /// duplicating it.
    fn into_iter(self) -> Self::IntoIter {
        let config = Arc::new(self.config.clone());
        Box::new(
            self.build_walker()
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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use crate::walker::test_utils::setup_test_dir;
    use std::fs;
    use std::path::Path;

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
    fn walk_iter_collects_entries() {
        let dir = setup_test_dir();
        let walker = CodeWalker::new(dir.path(), WalkConfig::default());
        let entries: Vec<FileEntry> = walker.walk_iter().collect::<Result<Vec<_>, _>>().unwrap();
        let paths: Vec<&Path> = entries.iter().map(|entry| entry.path.as_path()).collect();
        assert!(paths.iter().any(|p| p.ends_with("main.rs")));
        assert!(paths.iter().any(|p| p.ends_with("lib.rs")));
        assert!(paths.iter().any(|p| p.ends_with("src/app.py")));
    }

    #[cfg(unix)]
    #[test]
    fn handles_non_utf8_filenames() {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt;

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

    #[cfg(unix)]
    #[test]
    fn handles_permission_denied() {
        use std::os::unix::fs::PermissionsExt;

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
            assert!(
                !entries
                    .iter()
                    .any(|entry| entry.path.starts_with(&blocked_dir))
            );
        }
    }
}
