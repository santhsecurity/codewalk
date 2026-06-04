//! Shared file probing and enrichment primitives.

pub mod file_type;
pub mod format;
pub mod pe;

pub use hashkit::entropy::{entropy_bucket, shannon_entropy};
pub use file_type::{FileType, detect_file_type};
pub use format::{DecompressFormat, detect_format};
pub use pe::{PeMetadata, parse_pe};
