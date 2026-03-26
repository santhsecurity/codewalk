//! Load walk configuration from a TOML file.
//!
//! Run: cargo run --example toml_config

fn main() {
    // Parse config from inline TOML
    let config = codewalk::WalkConfig::from_toml(
        r#"
        max_file_size = 1048576
        skip_binary = true
        skip_hidden = true
        include_extensions = ["rs", "py", "js", "ts"]
        exclude_dirs = ["node_modules", ".git", "target", "__pycache__"]
    "#,
    )
    .expect("invalid TOML config");

    println!(
        "Config: max_file_size={}, skip_binary={}",
        config.max_file_size, config.skip_binary
    );
    println!("Extensions: {:?}", config.include_extensions);

    let walker = codewalk::CodeWalker::new(".", config);
    let entries = walker.walk().expect("failed to walk directory");
    println!("Found {} source files", entries.len());
}
