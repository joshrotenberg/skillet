use std::process::ExitCode;

use skillet_mcp::registry::{self, cache_dir_for_url, default_cache_dir};
use skillet_mcp::{git, repo};

use crate::ReposArgs;

/// Run the `repos` subcommand.
pub(crate) fn run_repos(_args: ReposArgs) -> ExitCode {
    // Always clone/pull the official registry to load the catalog.
    // This is independent of the user's configured registries -- the
    // catalog lives in the official repo.
    let cache_base = default_cache_dir();
    let target = cache_dir_for_url(&cache_base, registry::DEFAULT_REGISTRY_URL);
    if let Some(parent) = target.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Err(e) = git::clone_or_pull(registry::DEFAULT_REGISTRY_URL, &target) {
        eprintln!("Error loading official registry: {e}");
        return ExitCode::from(1);
    }

    let registry_path = target.join(registry::DEFAULT_REGISTRY_SUBDIR);
    let catalog = match repo::load_repos_catalog(&registry_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error loading repo catalog: {e}");
            return ExitCode::from(1);
        }
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
