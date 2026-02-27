//! Git operations for remote registry support.
//!
//! Shells out to the `git` CLI for clone, pull, and HEAD inspection.
//! No `git2`/`gix` dependency -- keeps things simple.

use std::path::Path;
use std::process::Command;

use crate::error::Error;

/// Clone a repository to the target directory (shallow by default).
pub fn clone(url: &str, target: &Path) -> crate::error::Result<()> {
    let output = Command::new("git")
        .args(["clone", "--depth", "1", url])
        .arg(target)
        .output()
        .map_err(|e| Error::Io {
            context: "failed to run git clone".to_string(),
            source: e,
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        return Err(Error::Git {
            operation: format!("clone {url}"),
            stderr,
        });
    }

    Ok(())
}

/// Pull latest changes in an existing clone.
pub fn pull(repo_path: &Path) -> crate::error::Result<()> {
    let output = Command::new("git")
        .args(["pull"])
        .current_dir(repo_path)
        .output()
        .map_err(|e| Error::Io {
            context: "failed to run git pull".to_string(),
            source: e,
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        return Err(Error::Git {
            operation: "pull".to_string(),
            stderr,
        });
    }

    Ok(())
}

/// Get the current HEAD commit hash.
pub fn head(repo_path: &Path) -> crate::error::Result<String> {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repo_path)
        .output()
        .map_err(|e| Error::Io {
            context: "failed to run git rev-parse HEAD".to_string(),
            source: e,
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        return Err(Error::Git {
            operation: "rev-parse HEAD".to_string(),
            stderr,
        });
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Clone if the target doesn't exist, otherwise pull.
pub fn clone_or_pull(url: &str, target: &Path) -> crate::error::Result<()> {
    if target.join(".git").exists() {
        tracing::info!(path = %target.display(), "Pulling existing clone");
        pull(target)
    } else {
        tracing::info!(url, path = %target.display(), "Cloning remote registry");
        clone(url, target)
    }
}
