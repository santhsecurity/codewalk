# codewalk

[![Crates.io](https://img.shields.io/crates/v/codewalk)](https://crates.io/crates/codewalk)
[![Docs.rs](https://docs.rs/codewalk/badge.svg)](https://docs.rs/codewalk)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE-MIT)

Part of [Santh](https://santh.dev) - open source Rust security and infrastructure tooling. Follow [@SanthProject](https://x.com/SanthProject) on X.

Walk a directory tree. Skip binaries, respect `.gitignore`, stream file contents in bounded chunks, and scan large trees in parallel.

```rust
use codewalk::{CodeWalker, WalkConfig};

let walker = CodeWalker::new("/path/to/repo", WalkConfig::default());
for entry in walker.walk().unwrap() {
    println!("{} ({} bytes)", entry.path.display(), entry.size);
    match entry.content().unwrap() {
        codewalk::FileContent::Text(text) => {
            // scan UTF-8 text
            let _ = text.len();
        }
        codewalk::FileContent::Binary(bytes) | codewalk::FileContent::Unknown(bytes) => {
            // handle raw bytes
            let _ = bytes.len();
        }
    }
}
```

## What the defaults do

Out of the box, codewalk skips:
- Binary files (detected by magic bytes, not just extension)
- Hidden files and directories
- Common junk directories: node_modules, .git, target, __pycache__, vendor, .venv, Pods
- Files over 10MB

It respects .gitignore rules automatically.

## Why not walkdir or ignore?

`walkdir` gives you paths. `ignore` gives you paths respecting gitignore. Neither reads file content, detects binary files by magic bytes, or offers a bounded chunked read path. If you're building a security scanner or code analyzer, you need all three: walk, skip binaries, read content efficiently. codewalk does that in one call. Without it you're stacking walkdir + a binary detector + a gitignore parser + chunked I/O + size limits. codewalk is that stack, tested and ready.

## Configuration

Override any default via struct fields or TOML:

```toml
max_file_size = 1048576
skip_binary = true
skip_hidden = false
respect_gitignore = true
follow_symlinks = false
include_extensions = ["rs", "py", "js"]
exclude_dirs = ["node_modules", ".git"]
```

```rust
let config = WalkConfig::from_toml(r#"
    include_extensions = ["rs", "py"]
    max_file_size = 5242880
"#).unwrap();
```

## Parallel walking

For large codebases, walk on multiple threads:

```rust
let rx = walker.walk_parallel(4);
for entry in rx {
    // entries arrive as they're discovered
}
```

## Content loading

`entry.content()` classifies content as `Text`, `Binary`, or `Unknown`. `entry.content_chunks()` streams the same file in bounded 64 KiB chunks when you need backpressure-friendly reads.

```rust
let content = entry.content().unwrap();
let bytes: &[u8] = content.as_bytes();

let chunks = entry
    .content_chunks()
    .unwrap()
    .collect::<codewalk::error::Result<Vec<_>>>()
    .unwrap();
assert!(chunks.iter().all(|chunk| chunk.len() <= 64 * 1024));
```

## Binary detection

Checks file extension first (fast), then magic bytes if needed. Recognizes ELF, PE, Mach-O, WASM, ZIP, images, audio, databases, and more.

## Contributing

Pull requests are welcome. There is no such thing as a perfect crate. If you find a bug, a better API, or just a rough edge, open a PR. We review quickly.

## License

MIT. Copyright 2026 CORUM COLLECTIVE LLC.

[![crates.io](https://img.shields.io/crates/v/codewalk.svg)](https://crates.io/crates/codewalk)
[![docs.rs](https://docs.rs/codewalk/badge.svg)](https://docs.rs/codewalk)
