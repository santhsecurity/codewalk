//! Basic usage: walk a directory and print file info.
//!
//! Run: cargo run --example basic -- /path/to/directory

fn main() {
    let root = std::env::args().nth(1).unwrap_or_else(|| ".".into());
    let config = codewalk::WalkConfig::default();
    let walker = codewalk::CodeWalker::new(&root, config);

    let entries = walker.walk().expect("failed to walk directory");
    println!("Found {} files in {}", entries.len(), root);

    for entry in &entries {
        let binary = if entry.is_binary { " [binary]" } else { "" };
        println!(
            "  {} ({} bytes){}",
            entry.path.display(),
            entry.size,
            binary
        );
    }
}
