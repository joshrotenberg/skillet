use std::process::ExitCode;

use skillet_mcp::{install, manifest, trust};

use super::parse_skill_ref;
use crate::UninstallArgs;

/// Run the `uninstall` subcommand.
pub(crate) fn run_uninstall(args: UninstallArgs) -> ExitCode {
    let (owner, name) = match parse_skill_ref(&args.skill) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Error: {e}");
            return ExitCode::from(1);
        }
    };

    let mut installed_manifest = match manifest::load() {
        Ok(m) => m,
        Err(e) => {
            eprintln!("Error loading installation manifest: {e}");
            return ExitCode::from(1);
        }
    };

    let opts = install::UninstallOptions::default();
    let result = match install::uninstall_skill(owner, name, &mut installed_manifest, &opts) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Error: {e}");
            return ExitCode::from(1);
        }
    };

    if let Err(e) = manifest::save(&installed_manifest) {
        eprintln!("Error saving installation manifest: {e}");
        return ExitCode::from(1);
    }

    // Unpin from trust state if requested
    if args.unpin {
        match trust::load() {
            Ok(mut trust_state) => {
                if trust_state.unpin_skill(owner, name)
                    && let Err(e) = trust::save(&trust_state)
                {
                    eprintln!("Warning: failed to save trust state: {e}");
                }
            }
            Err(e) => {
                eprintln!("Warning: failed to load trust state: {e}");
            }
        }
    }

    println!("Uninstalled {owner}/{name}");
    println!();
    for path in &result.removed_paths {
        println!("  removed {}", path.display());
    }

    ExitCode::SUCCESS
}
