//! Consolidated error types for the skillet library.
//!
//! Covers config, manifest, and install errors. Other modules (validate,
//! pack, publish, registry, index, integrity) continue to use `anyhow`
//! and can be migrated in a follow-up.

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

    // -- Generic --
    #[error("{context}: {source}")]
    Io {
        context: String,
        source: std::io::Error,
    },
    #[error("{0}")]
    Other(String),
}
