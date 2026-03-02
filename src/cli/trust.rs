use std::process::ExitCode;

use skillet_mcp::{config, integrity, manifest, repo, trust};

use super::parse_skill_ref;
use crate::{AuditArgs, TrustAction, TrustArgs, TrustPinArgs, TrustUnpinArgs};

/// Run the `trust` subcommand.
pub(crate) fn run_trust(args: TrustArgs) -> ExitCode {
    match args.action {
        TrustAction::List => run_trust_list(),
        TrustAction::Pin(a) => run_trust_pin(a),
        TrustAction::Unpin(a) => run_trust_unpin(a),
    }
}

pub(crate) fn run_trust_list() -> ExitCode {
    let state = match trust::load() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error loading trust state: {e}");
            return ExitCode::from(1);
        }
    };

    if state.pinned_skills.is_empty() {
        println!("No pinned skills.");
        return ExitCode::SUCCESS;
    }

    println!("Pinned skills ({}):\n", state.pinned_skills.len());
    for p in &state.pinned_skills {
        let hash_display = if p.content_hash.len() > 17 {
            format!("{}...", &p.content_hash[..17])
        } else {
            p.content_hash.clone()
        };
        println!(
            "  {}/{} v{}  {}  [{}]",
            p.owner, p.name, p.version, hash_display, p.pinned_at
        );
    }

    ExitCode::SUCCESS
}

pub(crate) fn run_trust_pin(args: TrustPinArgs) -> ExitCode {
    let (owner, name) = match parse_skill_ref(&args.skill) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Error: {e}");
            return ExitCode::from(1);
        }
    };

    let mut cli_config = match config::load_config() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error loading config: {e}");
            return ExitCode::from(1);
        }
    };

    if args.repos.no_cache {
        cli_config.cache.enabled = false;
    }

    let (skill_index, repo_paths) = match repo::load_repos(
        &args.repos.repo,
        &args.repos.remote,
        &cli_config,
        args.repos.subdir.as_deref(),
        args.repos.no_suggest,
    ) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Error loading repos: {e}");
            return ExitCode::from(1);
        }
    };

    let entry = match skill_index
        .skills
        .get(&(owner.to_string(), name.to_string()))
    {
        Some(e) => e,
        None => {
            eprintln!("Error: skill '{owner}/{name}' not found in any repo");
            return ExitCode::from(1);
        }
    };

    let version = match entry.latest() {
        Some(v) => v,
        None => {
            eprintln!("Error: no available versions for {owner}/{name} (all yanked)");
            return ExitCode::from(1);
        }
    };

    let repo_id = if !repo_paths.is_empty() {
        repo::repo_id(&repo_paths[0], &args.repos.remote)
    } else {
        "unknown".to_string()
    };

    let content_hash = integrity::sha256_hex(&version.skill_md);

    let mut trust_state = match trust::load() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error loading trust state: {e}");
            return ExitCode::from(1);
        }
    };

    trust_state.pin_skill(owner, name, &version.version, &repo_id, &content_hash);

    if let Err(e) = trust::save(&trust_state) {
        eprintln!("Error saving trust state: {e}");
        return ExitCode::from(1);
    }

    let hash_display = if content_hash.len() > 17 {
        format!("{}...", &content_hash[..17])
    } else {
        content_hash.clone()
    };
    println!(
        "Pinned {owner}/{name} v{} ({hash_display})",
        version.version
    );
    ExitCode::SUCCESS
}

pub(crate) fn run_trust_unpin(args: TrustUnpinArgs) -> ExitCode {
    let (owner, name) = match parse_skill_ref(&args.skill) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Error: {e}");
            return ExitCode::from(1);
        }
    };

    let mut state = match trust::load() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error loading trust state: {e}");
            return ExitCode::from(1);
        }
    };

    if !state.unpin_skill(owner, name) {
        eprintln!("Skill '{owner}/{name}' is not pinned.");
        return ExitCode::from(1);
    }

    if let Err(e) = trust::save(&state) {
        eprintln!("Error saving trust state: {e}");
        return ExitCode::from(1);
    }

    println!("Unpinned {owner}/{name}");
    ExitCode::SUCCESS
}

/// Run the `audit` subcommand.
pub(crate) fn run_audit(args: AuditArgs) -> ExitCode {
    let installed = match manifest::load() {
        Ok(m) => m,
        Err(e) => {
            eprintln!("Error loading installation manifest: {e}");
            return ExitCode::from(1);
        }
    };

    let trust_state = match trust::load() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error loading trust state: {e}");
            return ExitCode::from(1);
        }
    };

    let (filter_owner, filter_name) = if let Some(ref skill) = args.skill {
        match parse_skill_ref(skill) {
            Ok((o, n)) => (Some(o), Some(n)),
            Err(e) => {
                eprintln!("Error: {e}");
                return ExitCode::from(1);
            }
        }
    } else {
        (None, None)
    };

    let results = trust::audit(&installed, &trust_state, filter_owner, filter_name);

    if results.is_empty() {
        println!("No installed skills to audit.");
        return ExitCode::SUCCESS;
    }

    for r in &results {
        println!(
            "  [{status}] {owner}/{name} v{version} -> {path}",
            status = r.status,
            owner = r.owner,
            name = r.name,
            version = r.version,
            path = r.installed_to.display(),
        );
    }

    if trust::audit_has_problems(&results) {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}
