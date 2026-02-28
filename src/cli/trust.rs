use std::process::ExitCode;

use skillet_mcp::{config, integrity, manifest, registry, trust};

use super::parse_skill_ref;
use crate::{
    AuditArgs, TrustAction, TrustAddRegistryArgs, TrustArgs, TrustListArgs, TrustPinArgs,
    TrustRemoveRegistryArgs, TrustUnpinArgs,
};

/// Run the `trust` subcommand.
pub(crate) fn run_trust(args: TrustArgs) -> ExitCode {
    match args.action {
        TrustAction::AddRegistry(a) => run_trust_add_registry(a),
        TrustAction::RemoveRegistry(a) => run_trust_remove_registry(a),
        TrustAction::List(a) => run_trust_list(a),
        TrustAction::Pin(a) => run_trust_pin(a),
        TrustAction::Unpin(a) => run_trust_unpin(a),
    }
}

pub(crate) fn run_trust_add_registry(args: TrustAddRegistryArgs) -> ExitCode {
    let mut state = match trust::load() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error loading trust state: {e}");
            return ExitCode::from(1);
        }
    };

    if state.is_trusted(&args.url) {
        println!("Registry already trusted: {}", args.url);
        return ExitCode::SUCCESS;
    }

    state.add_registry(&args.url, args.note.as_deref());

    if let Err(e) = trust::save(&state) {
        eprintln!("Error saving trust state: {e}");
        return ExitCode::from(1);
    }

    println!("Trusted: {}", args.url);
    ExitCode::SUCCESS
}

pub(crate) fn run_trust_remove_registry(args: TrustRemoveRegistryArgs) -> ExitCode {
    let mut state = match trust::load() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error loading trust state: {e}");
            return ExitCode::from(1);
        }
    };

    if !state.remove_registry(&args.url) {
        eprintln!("Registry not found in trusted list: {}", args.url);
        return ExitCode::from(1);
    }

    if let Err(e) = trust::save(&state) {
        eprintln!("Error saving trust state: {e}");
        return ExitCode::from(1);
    }

    println!("Removed: {}", args.url);
    ExitCode::SUCCESS
}

pub(crate) fn run_trust_list(args: TrustListArgs) -> ExitCode {
    let state = match trust::load() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error loading trust state: {e}");
            return ExitCode::from(1);
        }
    };

    if state.trusted_registries.is_empty() && state.pinned_skills.is_empty() {
        println!("No trusted registries or pinned skills.");
        return ExitCode::SUCCESS;
    }

    if !state.trusted_registries.is_empty() {
        println!("Trusted registries ({}):\n", state.trusted_registries.len());
        for r in &state.trusted_registries {
            print!("  {}", r.registry);
            if let Some(ref note) = r.note {
                print!("  ({note})");
            }
            println!("  [{}]", r.trusted_at);
        }
    }

    if !args.registries_only && !state.pinned_skills.is_empty() {
        if !state.trusted_registries.is_empty() {
            println!();
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

    if args.registries.no_cache {
        cli_config.cache.enabled = false;
    }

    let (skill_index, registry_paths) = match registry::load_registries(
        &args.registries.registry,
        &args.registries.remote,
        &cli_config,
        args.registries.subdir.as_deref(),
    ) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Error loading registries: {e}");
            return ExitCode::from(1);
        }
    };

    let entry = match skill_index
        .skills
        .get(&(owner.to_string(), name.to_string()))
    {
        Some(e) => e,
        None => {
            eprintln!("Error: skill '{owner}/{name}' not found in any registry");
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

    let registry_id = if !registry_paths.is_empty() {
        registry::registry_id(&registry_paths[0], &args.registries.remote)
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

    trust_state.pin_skill(owner, name, &version.version, &registry_id, &content_hash);

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
