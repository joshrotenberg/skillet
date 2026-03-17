//! Consolidated error types for the skillet library.

use std::path::PathBuf;

/// Convenience alias used throughout the library.
pub type Result<T> = std::result::Result<T, Error>;

/// Unified error type for skillet library operations.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    // -- Config --
    #[error("failed to read config at {path}: {source}")]
    ConfigRead {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to parse config at {path}: {source}")]
    ConfigParse {
        path: PathBuf,
        source: toml::de::Error,
    },
    #[error("{0}")]
    Config(String),

    // -- Scaffold --
    #[error("{0}")]
    Scaffold(String),

    // -- Git --
    #[error("git {operation} failed: {stderr}")]
    Git { operation: String, stderr: String },

    // -- Repo --
    #[error("invalid duration: {0}")]
    InvalidDuration(String),

    // -- Index --
    #[error("failed to load skill at {path}: {reason}")]
    SkillLoad { path: PathBuf, reason: String },
    #[error("failed to parse {path}: {source}")]
    TomlParse {
        path: PathBuf,
        source: toml::de::Error,
    },
    #[error("failed to read {path}: {source}")]
    FileRead {
        path: PathBuf,
        source: std::io::Error,
    },

    // -- Generic --
    #[error("{context}: {source}")]
    Io {
        context: String,
        source: std::io::Error,
    },
    #[error("{0}")]
    Other(String),
}

/// Allow converting `std::io::Error` into `Error` for `?` in simple cases.
impl From<std::io::Error> for Error {
    fn from(source: std::io::Error) -> Self {
        Error::Io {
            context: "I/O error".to_string(),
            source,
        }
    }
}
