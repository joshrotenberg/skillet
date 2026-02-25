//! Git operations for remote registry support.
//!
//! Shells out to the `git` CLI for clone, pull, and HEAD inspection.
//! No `git2`/`gix` dependency -- keeps things simple.

use std::path::Path;
use std::process::Command;

use anyhow::{Context, bail};

/// Clone a repository to the target directory (shallow by default).
pub fn clone(url: &str, target: &Path) -> anyhow::Result<()> {
    let status = Command::new("git")
        .args(["clone", "--depth", "1", url])
        .arg(target)
        .status()
        .context("Failed to run git clone")?;

    if !status.success() {
        bail!("git clone failed with status {status}");
    }

    Ok(())
}

/// Pull latest changes in an existing clone.
pub fn pull(repo_path: &Path) -> anyhow::Result<()> {
    let status = Command::new("git")
        .args(["pull"])
        .current_dir(repo_path)
        .status()
        .context("Failed to run git pull")?;

    if !status.success() {
        bail!("git pull failed with status {status}");
    }

    Ok(())
}

/// Get the current HEAD commit hash.
pub fn head(repo_path: &Path) -> anyhow::Result<String> {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repo_path)
        .output()
        .context("Failed to run git rev-parse HEAD")?;

    if !output.status.success() {
        bail!("git rev-parse HEAD failed with status {}", output.status);
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Clone if the target doesn't exist, otherwise pull.
pub fn clone_or_pull(url: &str, target: &Path) -> anyhow::Result<()> {
    if target.join(".git").exists() {
        tracing::info!(path = %target.display(), "Pulling existing clone");
        pull(target)
    } else {
        tracing::info!(url, path = %target.display(), "Cloning remote registry");
        clone(url, target)
    }
}
