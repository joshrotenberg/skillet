use std::path::PathBuf;
use std::process::ExitCode;

use skillet_mcp::config;

use crate::{RepoAction, RepoCommand};

/// Run the `repo` subcommand.
pub(crate) fn run_repo(cmd: RepoCommand) -> ExitCode {
    match cmd.action {
        RepoAction::Add(args) => run_add(&args.repo),
        RepoAction::Remove(args) => run_remove(&args.repo),
        RepoAction::List => run_list(),
    }
}

fn run_add(repo: &str) -> ExitCode {
    let mut config = match config::load_config() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error loading config: {e}");
            return ExitCode::from(1);
        }
    };

    let added = if is_url(repo) {
        config::add_remote(&mut config, repo)
    } else {
        let path = PathBuf::from(repo);
        config::add_local(&mut config, &path)
    };

    if !added {
        eprintln!("Repo already configured: {repo}");
        return ExitCode::from(1);
    }

    match config::write_config(&config) {
        Ok(path) => {
            println!("Added repo: {repo}");
            println!("  config: {}", path.display());
        }
        Err(e) => {
            eprintln!("Error saving config: {e}");
            return ExitCode::from(1);
        }
    }

    ExitCode::SUCCESS
}

fn run_remove(repo: &str) -> ExitCode {
    let mut config = match config::load_config() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error loading config: {e}");
            return ExitCode::from(1);
        }
    };

    let removed = if is_url(repo) {
        config::remove_remote(&mut config, repo)
    } else {
        let path = PathBuf::from(repo);
        config::remove_local(&mut config, &path)
    };

    if !removed {
        eprintln!("Repo not found in config: {repo}");
        return ExitCode::from(1);
    }

    match config::write_config(&config) {
        Ok(path) => {
            println!("Removed repo: {repo}");
            println!("  config: {}", path.display());
        }
        Err(e) => {
            eprintln!("Error saving config: {e}");
            return ExitCode::from(1);
        }
    }

    ExitCode::SUCCESS
}

fn run_list() -> ExitCode {
    let config = match config::load_config() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error loading config: {e}");
            return ExitCode::from(1);
        }
    };

    let has_repos = !config.repos.remote.is_empty() || !config.repos.local.is_empty();

    if !has_repos {
        println!("No repos configured.");
        println!("  (the official repo is used by default)");
        return ExitCode::SUCCESS;
    }

    if !config.repos.remote.is_empty() {
        println!("Remote repos:");
        for url in &config.repos.remote {
            println!("  {url}");
        }
    }

    if !config.repos.local.is_empty() {
        if !config.repos.remote.is_empty() {
            println!();
        }
        println!("Local repos:");
        for path in &config.repos.local {
            println!("  {}", path.display());
        }
    }

    ExitCode::SUCCESS
}

/// Heuristic: treat as URL if it starts with http(s):// or git@.
fn is_url(s: &str) -> bool {
    s.starts_with("https://") || s.starts_with("http://") || s.starts_with("git@")
}
