//! WAVE2 (codewalk public-API invariants (WalkConfig + FileContent + detect)).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use proptest::prelude::*;

// ---- input-free invariants (plain unit tests) ----

#[test]
fn default_max_file_size_is_positive() {
    assert!(codewalk::WalkConfig::default().max_file_size > 0);
}

#[test]
fn artifact_defaults_has_no_exclude_dirs() {
    assert!(
        codewalk::WalkConfig::artifact_defaults()
            .exclude_dirs
            .is_empty()
    );
}

// ---- properties over arbitrary byte input ----

macro_rules! wave2_codewalk {
    ($($name:ident => |$bind:ident| $body:block),+ $(,)?) => {
        $(proptest! {
            #![proptest_config(ProptestConfig::with_cases(32))]
            #[test]
            fn $name($bind in prop::collection::vec(any::<u8>(), 0..256)) {
                $body
            }
        })+
    };
}

wave2_codewalk! {
    // Parsing arbitrary bytes as a config never panics.
    p00_from_toml_no_panic => |p| {
        let s = String::from_utf8_lossy(&p);
        let _ = codewalk::WalkConfig::from_toml(&s);
    },

    // Text content (valid UTF-8) classifies as text and round-trips both ways.
    p01_text_roundtrip => |p| {
        if let Ok(text) = String::from_utf8(p.clone()) {
            let fc = codewalk::FileContent::Text(text.clone());
            prop_assert!(fc.is_text());
            prop_assert!(!fc.is_binary());
            prop_assert!(!fc.is_unknown());
            prop_assert_eq!(fc.as_bytes(), text.as_bytes());
            prop_assert_eq!(fc.as_text(), Some(text.as_str()));
        }
    },

    // Unknown content is non-text, exposes no string, but preserves its bytes.
    p02_unknown_is_non_text => |p| {
        let fc = codewalk::FileContent::Unknown(p.clone());
        prop_assert!(fc.is_unknown());
        prop_assert!(!fc.is_text());
        prop_assert!(!fc.is_binary());
        prop_assert!(fc.as_text().is_none());
        prop_assert_eq!(fc.as_bytes(), p.as_slice());
    },

    // Binary content is non-text, exposes no string, but preserves its bytes.
    p03_binary_is_non_text => |p| {
        let fc = codewalk::FileContent::Binary(p.clone());
        prop_assert!(fc.is_binary());
        prop_assert!(!fc.is_text());
        prop_assert!(!fc.is_unknown());
        prop_assert!(fc.as_text().is_none());
        prop_assert_eq!(fc.as_bytes(), p.as_slice());
    },

    // The three classifications are mutually exclusive: exactly one holds.
    p04_classification_is_exclusive => |p| {
        for fc in [
            codewalk::FileContent::Binary(p.clone()),
            codewalk::FileContent::Unknown(p.clone()),
        ] {
            let set = [fc.is_text(), fc.is_binary(), fc.is_unknown()];
            prop_assert_eq!(set.iter().filter(|b| **b).count(), 1);
        }
    },

    // The builder records the max-file-size it is given.
    p05_builder_max_file_size_roundtrip => |p| {
        let size = p.len() as u64;
        let cfg = codewalk::WalkConfig::builder().max_file_size(size);
        prop_assert_eq!(cfg.max_file_size, size);
    },

    // Detecting an arbitrary on-disk file never panics.
    p06_is_binary_no_panic_on_disk => |p| {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("f.bin");
        std::fs::write(&path, &p).unwrap();
        let _ = codewalk::detect::is_binary(&path);
    },

    // `as_bytes` never changes the byte length of the content it wraps.
    p07_as_bytes_preserves_len => |p| {
        prop_assert_eq!(codewalk::FileContent::Binary(p.clone()).as_bytes().len(), p.len());
    },

    // Display renders the variant name, independent of payload.
    p08_display_is_variant_name => |p| {
        prop_assert_eq!(codewalk::FileContent::Binary(p.clone()).to_string(), "binary");
        prop_assert_eq!(codewalk::FileContent::Unknown(p.clone()).to_string(), "unknown");
    },
}
