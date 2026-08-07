# codewalk  -  Technical Spec

## Overview

Compatibility facade for scanner-oriented walking.  New code should depend on `walkkit` directly.

## Architecture

`codewalk` is a lightweight compatibility facade that re-exports core components from `walkkit`:

- Primary runner and config: `CodeWalker`, `WalkConfig`
- Entries and streaming: `FileEntry`, `FileContent`, `FileContentChunks`
- Submodules and helpers: `detect`, `error`, `probe`, `sandbox`

## Guarantees

- `#![forbid(unsafe_code)]` in force.
- Strict lint preamble: `unwrap_used`, `expect_used`, `todo`, `unimplemented`, and `panic` denied in non-test builds.
- Full backward compatibility for legacy `codewalk` callers while delegating implementation to `walkkit`.

## Public API Summary

Key entry points re-exported in `src/lib.rs` from `walkkit`:

- `CodeWalker`: Main scanner-oriented directory traversal struct.
- `WalkConfig`: Builder/configuration for traversal behavior, ignore rules, size bounds, and concurrency.
- `FileEntry`: Traversed file item metadata.
- `FileContent`: Categorized content enum (`Text`, `Binary`, `Unknown`).
- `FileContentChunks`: Bounded chunk iterator for streaming file content.
- `detect`, `error`, `probe`, `sandbox`: Re-exported submodules.

## Error Handling

Error types and `Result` aliases are provided via the re-exported `error` module (`walkkit::error`).
