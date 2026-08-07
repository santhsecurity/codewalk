#![warn(clippy::pedantic)]
#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::todo,
        clippy::unimplemented,
        clippy::panic
    )
)]
#![allow(
    clippy::module_name_repetitions,
    clippy::must_use_candidate,
    clippy::missing_errors_doc
)]
#![warn(missing_docs)]
#![forbid(unsafe_code)]
//! Compatibility facade for scanner-oriented walking.
//!
//! New code should depend on `walkkit` directly.

pub use walkkit::{
    CodeWalker, Error, FileContent, FileContentChunks, FileEntry, MAX_WALK_PATH_BYTES, WalkConfig,
    WalkError, WalkItem, WalkOp, WalkedFile, Walker, detect, error, filter, probe, sandbox,
};
