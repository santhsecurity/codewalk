//! Example using ignore patterns.
//!
//! Run: cargo run --example ignore_patterns -- /path

use codewalk::{CodeWalker, WalkConfig};

fn main() {
    let root = std::env::args().nth(1).unwrap_or_else(|| ".".into());
    // Create a walker to demonstrate scanning a directory while
    // potentially respecting gitignore or custom ignore rules.
    let config = WalkConfig::default();
    let walker = CodeWalker::new(&root, config);

    let entries = walker.walk().expect("failed to walk directory");
    println!("Walked {} files.", entries.len());

    // Display up to 5 files to show it worked
    for entry in entries.iter().take(5) {
        println!("  {}", entry.path.display());
    }
}
