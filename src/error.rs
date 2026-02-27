//! Consolidated error types for the skillet library.
//!
//! All library modules use `crate::error::{Error, Result}`. The binary
//! crate (`main.rs`) and the private `discover::build_local_entry` helper
//! still use `anyhow` where appropriate.

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

    // -- Manifest --
    #[error("failed to read manifest at {path}: {source}")]
    ManifestRead {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to parse manifest at {path}: {source}")]
    ManifestParse {
        path: PathBuf,
        source: toml::de::Error,
    },
    #[error("failed to write manifest to {path}: {source}")]
    ManifestWrite {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to serialize manifest: {0}")]
    ManifestSerialize(toml::ser::Error),

    // -- Trust --
    #[error("failed to read trust state at {path}: {source}")]
    TrustRead {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to parse trust state at {path}: {source}")]
    TrustParse {
        path: PathBuf,
        source: toml::de::Error,
    },
    #[error("failed to write trust state to {path}: {source}")]
    TrustWrite {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to serialize trust state: {0}")]
    TrustSerialize(toml::ser::Error),

    // -- Install --
    #[error("failed to create directory {path}: {source}")]
    CreateDir {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to write {path}: {source}")]
    WriteFile {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to resolve current directory: {0}")]
    CurrentDir(std::io::Error),

    // -- Integrity --
    #[error("invalid manifest line (expected two-space separator): {line}")]
    ManifestFormatError { line: String },
    #[error("MANIFEST.sha256 missing composite hash")]
    ManifestMissingComposite,

    // -- Scaffold --
    #[error("{0}")]
    Scaffold(String),

    // -- Git --
    #[error("git {operation} failed: {stderr}")]
    Git { operation: String, stderr: String },

    // -- Registry --
    #[error("invalid duration: {0}")]
    InvalidDuration(String),

    // -- Validate --
    #[error("validation error: {0}")]
    Validation(String),

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

    // -- Publish --
    #[error("{0}")]
    Publish(String),

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
