#![allow(clippy::unwrap_used)]

use codewalk::detect::is_binary;
use codewalk::{FileContent, WalkConfig};
use proptest::prelude::*;
use std::fs;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]

    #[test]
    fn test_walkconfig_from_toml_does_not_panic(s in "\\PC*") {
        let _ = WalkConfig::from_toml(&s);
    }

    #[test]
    fn test_detect_is_binary_does_not_panic(bytes in any::<Vec<u8>>(), ext in "[a-zA-Z0-9]{0,5}") {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(format!("test.{ext}"));
        fs::write(&path, &bytes).unwrap();

        let _ = is_binary(&path);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    #[test]
    fn test_filecontent_invariants(bytes in any::<Vec<u8>>()) {
        // If it's valid UTF-8, FileContent::Text should be created.
        if let Ok(text) = String::from_utf8(bytes.clone()) {
            let content = FileContent::Text(text.clone());
            assert!(content.is_text());
            assert!(!content.is_binary());
            assert!(!content.is_unknown());
            assert_eq!(content.as_text().unwrap(), text);
            assert_eq!(content.as_bytes(), bytes.as_slice());
        } else {
            let content = FileContent::Unknown(bytes.clone());
            assert!(content.is_unknown());
            assert!(!content.is_text());
            assert!(!content.is_binary());
            assert!(content.as_text().is_none());
            assert_eq!(content.as_bytes(), bytes.as_slice());
        }

        let binary_content = FileContent::Binary(bytes.clone());
        assert!(binary_content.is_binary());
        assert!(!binary_content.is_text());
        assert!(!binary_content.is_unknown());
        assert!(binary_content.as_text().is_none());
        assert_eq!(binary_content.as_bytes(), bytes.as_slice());
    }
}
