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

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use tempfile::TempDir;

    /// Create a bare git repo and return its temp directory.
    fn make_bare_repo() -> TempDir {
        let dir = TempDir::new().unwrap();
        Command::new("git")
            .args(["init", "--bare"])
            .arg(dir.path())
            .output()
            .unwrap();
        dir
    }

    /// Create a git repo with an initial commit and return its temp directory.
    fn make_repo_with_commit() -> TempDir {
        let dir = TempDir::new().unwrap();
        let p = dir.path();
        Command::new("git").args(["init"]).arg(p).output().unwrap();
        Command::new("git")
            .args([
                "-C",
                &p.display().to_string(),
                "config",
                "user.email",
                "test@test.com",
            ])
            .output()
            .unwrap();
        Command::new("git")
            .args([
                "-C",
                &p.display().to_string(),
                "config",
                "user.name",
                "Test",
            ])
            .output()
            .unwrap();
        std::fs::write(p.join("README.md"), "# Test").unwrap();
        Command::new("git")
            .args(["-C", &p.display().to_string(), "add", "."])
            .output()
            .unwrap();
        Command::new("git")
            .args([
                "-C",
                &p.display().to_string(),
                "-c",
                "commit.gpgsign=false",
                "commit",
                "-m",
                "init",
            ])
            .output()
            .unwrap();
        dir
    }

    #[test]
    fn head_returns_commit_hash() {
        let repo = make_repo_with_commit();
        let hash = head(repo.path()).unwrap();
        assert_eq!(hash.len(), 40);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn head_fails_on_non_repo() {
        let dir = TempDir::new().unwrap();
        let result = head(dir.path());
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("rev-parse HEAD"));
    }

    #[test]
    fn clone_from_local_bare() {
        let bare = make_bare_repo();
        let target = TempDir::new().unwrap();
        let dest = target.path().join("clone");
        let result = clone(&format!("file://{}", bare.path().display()), &dest);
        assert!(result.is_ok());
        assert!(dest.join(".git").exists());
    }

    #[test]
    fn clone_fails_with_bad_url() {
        let target = TempDir::new().unwrap();
        let dest = target.path().join("clone");
        let result = clone("file:///nonexistent/repo", &dest);
        assert!(result.is_err());
    }

    #[test]
    fn pull_on_valid_repo() {
        let origin = make_repo_with_commit();
        let target = TempDir::new().unwrap();
        let dest = target.path().join("clone");
        clone(&format!("file://{}", origin.path().display()), &dest).unwrap();
        let result = pull(&dest);
        assert!(result.is_ok());
    }

    #[test]
    fn pull_fails_on_non_repo() {
        let dir = TempDir::new().unwrap();
        let result = pull(dir.path());
        assert!(result.is_err());
    }

    #[test]
    fn clone_or_pull_clones_when_no_git() {
        let bare = make_bare_repo();
        let target = TempDir::new().unwrap();
        let dest = target.path().join("clone");
        let result = clone_or_pull(&format!("file://{}", bare.path().display()), &dest);
        assert!(result.is_ok());
        assert!(dest.join(".git").exists());
    }

    #[test]
    fn clone_or_pull_pulls_when_git_exists() {
        let origin = make_repo_with_commit();
        let target = TempDir::new().unwrap();
        let dest = target.path().join("clone");
        clone(&format!("file://{}", origin.path().display()), &dest).unwrap();
        let result = clone_or_pull(&format!("file://{}", origin.path().display()), &dest);
        assert!(result.is_ok());
    }
}
