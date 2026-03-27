use std::io;

/// Errors that can occur during a code walk.
#[derive(thiserror::Error, Debug)]
pub enum CodewalkError {
    /// An I/O error occurred while reading the file system.
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    /// A file is too large to process.
    #[error("File too large: {0} bytes")]
    FileTooLarge(u64),

    /// An ignore rule or walk configuration error.
    #[error("Walk builder error: {0}")]
    Ignore(#[from] ignore::Error),

    /// A UTF-8 decoding error occurred.
    #[error("UTF-8 decoding error: {0}")]
    Utf8Error(#[from] std::string::FromUtf8Error),
}

/// A specialized Result type for codewalk.
pub type Result<T> = std::result::Result<T, CodewalkError>;
