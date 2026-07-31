# Changelog

## [0.3.3] - 2026-07-30

### Fixed
- Require `walkkit` 0.1.1 so `FileEntry::content` re-stats the live file and
  reads to EOF instead of silently truncating files that grow between `walk()`
  and `content()` (unblocks `gap_test_toctou_large_file_replacement`).

## [0.3.0] - 2026-04-12

### Added
- Initial release of codewalk.
