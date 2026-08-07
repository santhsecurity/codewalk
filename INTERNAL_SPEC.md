# codewalk  -  Internal Spec

> This file is gitignored. It exists for agents and internal development. Never committed to public repos.

## Identity
Compatibility facade for scanner-oriented file tree walking with binary detection and bounded reads.

## Purpose
Provides a safe interface for directory traversal and file reading. Without it, tools would reinvent directory traversal and file content reading logic, risking infinite symlink loops or memory exhaustion on large binaries.

## North Star
Zero-allocation, high-throughput file discovery that flawlessly skips unreadable or malicious file system structures and delegates efficiently to `walkkit`.

## Role in Ecosystem
- **Depends on:** walkkit
- **Depended on by:** warpscan, internal local scanning tools
- **Relationship to warpscan:** Used during local repository scans to efficiently discover and filter target source files.
- **Standalone value:** YES. Useful for any Rust tool that needs fast, safe, and filtered directory traversal, though users are encouraged to use `walkkit` directly.

## Invariants
Never follows infinite symlink loops. Never reads infinitely large files into memory.

## Boundaries
Does not analyze the content (no AST parsing or regex matching)  -  only discovers and safely reads chunks.

## Quality State
- Tests: Comprehensive (includes proptest)
- Lint preamble: yes
- #![forbid(unsafe_code)]: yes
- Doc coverage: ~80%
- Known issues: Deprecated in favor of direct `walkkit` usage.