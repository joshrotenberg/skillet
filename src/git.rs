//! Git operations for remote repo support.
//!
//! Shells out to the `git` CLI for clone, pull, and HEAD inspection.
//! No `git2`/`gix` dependency -- keeps things simple.

use std::path::Path;
use std::process::Command;
use std::time::Duration;

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

/// List tags in a local clone, sorted by version (latest last).
///
/// Returns tag names (e.g. `["v0.1.0", "v0.2.0", "v1.0.0"]`).
/// Returns an empty vec if the repo has no tags.
pub fn list_tags(repo_path: &Path) -> crate::error::Result<Vec<String>> {
    let output = Command::new("git")
        .args(["tag", "--list", "--sort=version:refname"])
        .current_dir(repo_path)
        .output()
        .map_err(|e| Error::Io {
            context: "failed to run git tag".to_string(),
            source: e,
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        return Err(Error::Git {
            operation: "tag --list".to_string(),
            stderr,
        });
    }

    let tags: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| l.to_string())
        .collect();

    Ok(tags)
}

/// Fetch tags from the remote (needed for shallow clones).
pub fn fetch_tags(repo_path: &Path) -> crate::error::Result<()> {
    let output = Command::new("git")
        .args(["fetch", "--tags", "--quiet"])
        .current_dir(repo_path)
        .output()
        .map_err(|e| Error::Io {
            context: "failed to run git fetch --tags".to_string(),
            source: e,
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        return Err(Error::Git {
            operation: "fetch --tags".to_string(),
            stderr,
        });
    }

    Ok(())
}

/// Checkout a specific ref (tag, branch, or commit).
pub fn checkout(repo_path: &Path, ref_name: &str) -> crate::error::Result<()> {
    let output = Command::new("git")
        .args(["checkout", ref_name, "--quiet"])
        .current_dir(repo_path)
        .output()
        .map_err(|e| Error::Io {
            context: format!("failed to run git checkout {ref_name}"),
            source: e,
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        return Err(Error::Git {
            operation: format!("checkout {ref_name}"),
            stderr,
        });
    }

    Ok(())
}

/// Clone if the target doesn't exist, otherwise pull.
pub fn clone_or_pull(url: &str, target: &Path) -> crate::error::Result<()> {
    if target.join(".git").exists() {
        tracing::info!(path = %target.display(), "Pulling existing clone");
        pull(target)
    } else {
        tracing::info!(url, path = %target.display(), "Cloning remote repo");
        clone(url, target)
    }
}

/// Clone or pull with a timeout for git HTTP operations.
///
/// Sets `GIT_HTTP_LOW_SPEED_LIMIT` and `GIT_HTTP_LOW_SPEED_TIME` environment
/// variables to make git itself abort slow connections. This avoids needing
/// process kill logic and works for both clone and pull.
///
/// Note: these env vars only affect HTTP(S) transports; SSH clones use
/// the SSH client's own timeout configuration.
pub fn clone_or_pull_with_timeout(
    url: &str,
    target: &Path,
    timeout: Duration,
) -> crate::error::Result<()> {
    let timeout_secs = timeout.as_secs().max(1).to_string();

    if target.join(".git").exists() {
        tracing::info!(path = %target.display(), "Pulling existing clone (with timeout)");
        let output = Command::new("git")
            .args(["pull"])
            .current_dir(target)
            .env("GIT_HTTP_LOW_SPEED_LIMIT", "1000")
            .env("GIT_HTTP_LOW_SPEED_TIME", &timeout_secs)
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
    } else {
        tracing::info!(url, path = %target.display(), "Cloning remote repo (with timeout)");
        let output = Command::new("git")
            .args(["clone", "--depth", "1", url])
            .arg(target)
            .env("GIT_HTTP_LOW_SPEED_LIMIT", "1000")
            .env("GIT_HTTP_LOW_SPEED_TIME", &timeout_secs)
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
}

#[cfg(test)]
pub(crate) mod tests {
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

    /// Public version for use by other test modules.
    pub(crate) fn make_repo_with_commit_pub() -> TempDir {
        make_repo_with_commit()
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
