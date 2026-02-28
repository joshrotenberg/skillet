use std::process::ExitCode;

use skillet_mcp::{config, registry, repo};

use crate::ReposArgs;

/// Run the `repos` subcommand.
pub(crate) fn run_repos(args: ReposArgs) -> ExitCode {
    let mut cli_config = match config::load_config() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error loading config: {e}");
            return ExitCode::from(1);
        }
    };

    if args.no_cache {
        cli_config.cache.enabled = false;
    }

    // Load the official registry to get the catalog
    let (_, registry_paths) =
        match registry::load_registries(&[], &[], &cli_config, args.subdir.as_deref()) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("Error loading registries: {e}");
                return ExitCode::from(1);
            }
        };

    // Load catalog from the first registry path
    let catalog = if let Some(path) = registry_paths.first() {
        match repo::load_repos_catalog(path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Error loading repo catalog: {e}");
                return ExitCode::from(1);
            }
        }
    } else {
        eprintln!("No registry loaded, cannot read repo catalog.");
        return ExitCode::from(1);
    };

    if catalog.is_empty() {
        println!("No external skill repos found in the catalog.");
        return ExitCode::SUCCESS;
    }

    println!(
        "{} external skill repo{} available:\n",
        catalog.len(),
        if catalog.len() == 1 { "" } else { "s" }
    );

    for entry in catalog.entries() {
        println!("  {}", entry.name);
        if let Some(ref desc) = entry.description {
            println!("    {desc}");
        }
        if let Some(ref domains) = entry.domains {
            println!("    domains: {}", domains.join(", "));
        }
        if let Some(ref subdir) = entry.subdir {
            println!("    subdir: {subdir}");
        }
        println!();
    }

    println!("Add to your config:");
    println!("  skillet setup --repos anthropics/skills,vercel-labs/agent-skills");
    println!("\nOr one-time:");
    println!("  skillet search react --repo vercel-labs/agent-skills");

    ExitCode::SUCCESS
}
