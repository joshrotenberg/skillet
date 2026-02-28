use std::process::ExitCode;

use skillet_mcp::install::{self, InstallOptions};
use skillet_mcp::{config, integrity, manifest, registry, safety, trust};

use super::{parse_skill_ref, print_safety_report};
use crate::InstallArgs;

/// Run the `install` subcommand.
pub(crate) fn run_install(args: InstallArgs) -> ExitCode {
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

    let targets = match config::resolve_targets(&args.target, &cli_config) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("Error: {e}");
            return ExitCode::from(1);
        }
    };

    let global = args.global || cli_config.install.global;

    let (skill_index, registry_paths, _catalog) = match registry::load_registries_with_repos(
        &args.registries.registry,
        &args.registries.remote,
        &args.registries.repo,
        &cli_config,
        args.registries.subdir.as_deref(),
    ) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Error loading registries: {e}");
            return ExitCode::from(1);
        }
    };

    // Look up the skill
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

    // Resolve version
    let version = if let Some(ref requested) = args.version {
        match entry.versions.iter().find(|v| v.version == *requested) {
            Some(v) if !v.has_content => {
                eprintln!(
                    "Error: version {requested} exists but content is not available \
                     (only the latest version has full content)"
                );
                return ExitCode::from(1);
            }
            Some(v) => v,
            None => {
                let available: Vec<&str> =
                    entry.versions.iter().map(|v| v.version.as_str()).collect();
                eprintln!(
                    "Error: version '{requested}' not found for {owner}/{name}\n\
                     Available versions: {}",
                    available.join(", ")
                );
                return ExitCode::from(1);
            }
        }
    } else {
        match entry.latest() {
            Some(v) => v,
            None => {
                eprintln!("Error: no available versions for {owner}/{name} (all yanked)");
                return ExitCode::from(1);
            }
        }
    };

    // Load manifest
    let mut installed_manifest = match manifest::load() {
        Ok(m) => m,
        Err(e) => {
            eprintln!("Error loading installation manifest: {e}");
            return ExitCode::from(1);
        }
    };

    // Determine registry identifier
    let registry_id = if !registry_paths.is_empty() {
        registry::registry_id(&registry_paths[0], &args.registries.remote)
    } else {
        "unknown".to_string()
    };

    // Trust checking
    let content_hash = integrity::sha256_hex(&version.skill_md);
    let trust_state = match trust::load() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error loading trust state: {e}");
            return ExitCode::from(1);
        }
    };

    let trust_check = trust::check_trust(&trust_state, &registry_id, owner, name, &content_hash);

    match trust_check.tier {
        trust::TrustTier::Trusted => {}
        trust::TrustTier::Reviewed => {
            if trust_check.pinned_hash.as_deref() != Some(&content_hash) {
                eprintln!(
                    "Warning: {} (tier: reviewed, content changed since pinned)",
                    trust_check.reason
                );
            }
        }
        trust::TrustTier::Unknown => {
            // --require-trusted flag or config overrides policy
            if args.require_trusted || cli_config.trust.require_trusted {
                eprintln!(
                    "Error: {reason}\n\n\
                     Install blocked: --require-trusted is set.\n\
                     To install this skill, either:\n\
                     \n  1. Trust the registry:\n\
                     \n     skillet trust add-registry {registry_id}\n\
                     \n  2. Review and pin the skill:\n\
                     \n     skillet info {owner}/{name}\n\
                     \n     skillet trust pin {owner}/{name}\n\
                     \n     skillet install {owner}/{name}\n",
                    reason = trust_check.reason,
                );
                return ExitCode::from(1);
            }

            let policy = &cli_config.trust.unknown_policy;
            match policy.as_str() {
                "block" => {
                    eprintln!(
                        "Error: {reason}\n\n\
                         Install blocked by trust policy (unknown_policy = \"block\").\n\
                         To install this skill, either:\n\
                         \n  1. Trust the registry:\n\
                         \n     skillet trust add-registry {registry_id}\n\
                         \n  2. Review and pin the skill:\n\
                         \n     skillet info {owner}/{name}\n\
                         \n     skillet trust pin {owner}/{name}\n\
                         \n     skillet install {owner}/{name}\n",
                        reason = trust_check.reason,
                    );
                    return ExitCode::from(1);
                }
                "prompt" => {
                    eprintln!(
                        "Warning: {}\nProceed with install? [y/N] ",
                        trust_check.reason
                    );
                    let mut input = String::new();
                    if std::io::stdin().read_line(&mut input).is_err()
                        || !input.trim().eq_ignore_ascii_case("y")
                    {
                        eprintln!("Install cancelled.");
                        return ExitCode::from(1);
                    }
                }
                _ => {
                    // "warn" (default) -- explicit guidance
                    eprintln!(
                        "Warning: {reason}\n\
                         To verify before installing:\n\
                         \n  skillet info {owner}/{name}\n\
                         \n  skillet trust pin {owner}/{name}\n",
                        reason = trust_check.reason,
                    );
                }
            }
        }
    }

    let options = InstallOptions {
        targets,
        global,
        registry: registry_id.clone(),
    };

    let results =
        match install::install_skill(owner, name, version, &options, &mut installed_manifest) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("Error installing skill: {e}");
                return ExitCode::from(1);
            }
        };

    // Save manifest
    if let Err(e) = manifest::save(&installed_manifest) {
        eprintln!("Error saving installation manifest: {e}");
        return ExitCode::from(1);
    }

    // Auto-pin content hash after successful install
    if cli_config.trust.auto_pin {
        let mut trust_state = trust_state;
        trust_state.pin_skill(owner, name, &version.version, &registry_id, &content_hash);
        if let Err(e) = trust::save(&trust_state) {
            eprintln!("Warning: failed to save trust state: {e}");
        }
    }

    // Print results
    println!(
        "Installed {owner}/{name} v{version}",
        version = version.version
    );
    println!();
    for r in &results {
        let file_count = r.files_written.len();
        let scope = if options.global { "global" } else { "project" };
        println!(
            "  {target} ({scope}) ... {file_count} file{s} -> {path}",
            target = r.target,
            s = if file_count == 1 { "" } else { "s" },
            path = r.path.display(),
        );
    }

    // Safety scanning (informational only -- never blocks install)
    let report = safety::scan(
        &version.skill_md,
        &version.skill_toml_raw,
        &version.files,
        &version.metadata,
        &cli_config.safety.suppress,
    );

    if !report.is_empty() {
        println!();
        print_safety_report(&report);
    }

    ExitCode::SUCCESS
}
