# codewalk

Walk a directory tree. Skip binaries, respect .gitignore, memory-map large files. Parallel mode for scanning big codebases.

```rust
use codewalk::{CodeWalker, WalkConfig};

let walker = CodeWalker::new("/path/to/repo", WalkConfig::default());
for entry in walker.walk() {
    println!("{} ({} bytes)", entry.path.display(), entry.size);
    let content = entry.content_str().unwrap();
    // scan the content
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

`walkdir` gives you paths. `ignore` gives you paths respecting gitignore. Neither reads file content, detects binary files by magic bytes, or memory-maps large files. If you're building a security scanner or code analyzer, you need all three: walk, skip binaries, read content efficiently. codewalk does that in one call. Without it you're stacking walkdir + a binary detector + a gitignore parser + an mmap wrapper + size limits. codewalk is that stack, tested and ready.

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

## Memory-mapped reading

Files above 64KB (configurable) are memory-mapped instead of read into a Vec. Below that threshold, regular read is faster.

```rust
let content = entry.content().unwrap();
let bytes: &[u8] = content.as_bytes();
```

## Binary detection

Checks file extension first (fast), then magic bytes if needed. Recognizes ELF, PE, Mach-O, WASM, ZIP, images, audio, databases, and more.

## Contributing

Pull requests are welcome. There is no such thing as a perfect crate. If you find a bug, a better API, or just a rough edge, open a PR. We review quickly.

## License

MIT. Copyright 2026 CORUM COLLECTIVE LLC.

[![crates.io](https://img.shields.io/crates/v/codewalk.svg)](https://crates.io/crates/codewalk)
[![docs.rs](https://docs.rs/codewalk/badge.svg)](https://docs.rs/codewalk)
