use std::process::ExitCode;

use skillet_mcp::{config, manifest, registry, search, state};

use super::parse_skill_ref;
use crate::{CategoriesArgs, InfoArgs, ListArgs, SearchArgs};

/// Run the `search` subcommand.
pub(crate) fn run_search(args: SearchArgs) -> ExitCode {
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

    let (skill_index, _registry_paths) = match registry::load_registries(
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

    let skill_search = search::SkillSearch::build(&skill_index);

    // Wildcard: list all skills
    let results: Vec<state::SkillSummary> = if args.query == "*" {
        let mut keys: Vec<_> = skill_index.skills.keys().collect();
        keys.sort();
        keys.iter()
            .filter_map(|k| {
                let entry = skill_index.skills.get(*k)?;
                state::SkillSummary::from_entry(entry)
            })
            .collect()
    } else {
        let hits = skill_search.search(&args.query, 20);
        hits.iter()
            .filter_map(|(owner, name, _score)| {
                let entry = skill_index.skills.get(&(owner.clone(), name.clone()))?;
                state::SkillSummary::from_entry(entry)
            })
            .collect()
    };

    // Apply structured filters
    let results: Vec<_> = results
        .into_iter()
        .filter(|s| {
            if let Some(ref cat) = args.category
                && !s.categories.iter().any(|c| c.eq_ignore_ascii_case(cat))
            {
                return false;
            }
            if let Some(ref tag) = args.tag
                && !s.tags.iter().any(|t| t.eq_ignore_ascii_case(tag))
            {
                return false;
            }
            if let Some(ref owner) = args.owner
                && !s.owner.eq_ignore_ascii_case(owner)
            {
                return false;
            }
            true
        })
        .collect();

    if results.is_empty() {
        println!("No skills found.");
        return ExitCode::SUCCESS;
    }

    println!(
        "Found {} skill{}:\n",
        results.len(),
        if results.len() == 1 { "" } else { "s" }
    );
    for s in &results {
        println!("  {}/{} v{}", s.owner, s.name, s.version);
        println!("    {}", s.description);
        if !s.categories.is_empty() {
            println!("    categories: {}", s.categories.join(", "));
        }
        if !s.tags.is_empty() {
            println!("    tags: {}", s.tags.join(", "));
        }
        println!();
    }

    ExitCode::SUCCESS
}

/// Run the `categories` subcommand.
pub(crate) fn run_categories(args: CategoriesArgs) -> ExitCode {
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

    let (skill_index, _registry_paths) = match registry::load_registries(
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

    if skill_index.categories.is_empty() {
        println!("No categories found.");
        return ExitCode::SUCCESS;
    }

    let total: usize = skill_index.categories.values().sum();
    println!(
        "{} categor{} ({total} skill{}):\n",
        skill_index.categories.len(),
        if skill_index.categories.len() == 1 {
            "y"
        } else {
            "ies"
        },
        if total == 1 { "" } else { "s" },
    );
    for (name, count) in &skill_index.categories {
        println!("  {name} ({count})");
    }

    ExitCode::SUCCESS
}

/// Run the `info` subcommand.
pub(crate) fn run_info(args: InfoArgs) -> ExitCode {
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

    let (skill_index, _registry_paths) = match registry::load_registries(
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

    let latest = match entry.latest() {
        Some(v) => v,
        None => {
            eprintln!("Error: no available versions for {owner}/{name} (all yanked)");
            return ExitCode::from(1);
        }
    };

    let info = &latest.metadata.skill;

    println!("{owner}/{name}\n");
    println!("  version ............... {}", info.version);
    println!("  description ........... {}", info.description);

    if let Some(ref trigger) = info.trigger {
        println!("  trigger ............... {trigger}");
    }
    if let Some(ref license) = info.license {
        println!("  license ............... {license}");
    }
    if let Some(ref author) = info.author {
        if let Some(ref name) = author.name {
            println!("  author ................ {name}");
        }
        if let Some(ref github) = author.github {
            println!("  github ................ {github}");
        }
    }
    if let Some(ref classification) = info.classification {
        if !classification.categories.is_empty() {
            println!(
                "  categories ............ {}",
                classification.categories.join(", ")
            );
        }
        if !classification.tags.is_empty() {
            println!(
                "  tags .................. {}",
                classification.tags.join(", ")
            );
        }
    }
    if let Some(ref compat) = info.compatibility
        && !compat.verified_with.is_empty()
    {
        println!(
            "  verified with ......... {}",
            compat.verified_with.join(", ")
        );
    }

    // Extra files
    if !latest.files.is_empty() {
        let mut file_paths: Vec<&String> = latest.files.keys().collect();
        file_paths.sort();
        println!(
            "  files ................. {}",
            file_paths
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }

    // Published timestamp
    if let Some(ref published) = latest.published {
        println!("  published ............. {published}");
    }

    // Content hash
    if let Some(ref hash) = latest.content_hash {
        let display = if hash.len() > 17 {
            format!("{}...", &hash[..17])
        } else {
            hash.clone()
        };
        println!("  content hash .......... {display}");
    }

    // Integrity
    match latest.integrity_ok {
        Some(true) => println!("  integrity ............. verified"),
        Some(false) => println!("  integrity ............. MISMATCH"),
        None => {}
    }

    // Version history
    let available: Vec<&str> = entry
        .versions
        .iter()
        .filter(|v| !v.yanked)
        .map(|v| v.version.as_str())
        .collect();
    if available.len() > 1 {
        println!("  versions .............. {}", available.join(", "));
    }

    // Registry path for nested skills
    if let Some(ref rpath) = entry.registry_path {
        println!("  registry path ......... {rpath}");
    }

    ExitCode::SUCCESS
}

/// Run the `list` subcommand.
pub(crate) fn run_list(_args: ListArgs) -> ExitCode {
    let installed_manifest = match manifest::load() {
        Ok(m) => m,
        Err(e) => {
            eprintln!("Error loading installation manifest: {e}");
            return ExitCode::from(1);
        }
    };

    if installed_manifest.skills.is_empty() {
        println!("No skills installed.");
        return ExitCode::SUCCESS;
    }

    // Group by (owner, name)
    let mut groups: std::collections::BTreeMap<(String, String), Vec<&manifest::InstalledSkill>> =
        std::collections::BTreeMap::new();
    for skill in &installed_manifest.skills {
        groups
            .entry((skill.owner.clone(), skill.name.clone()))
            .or_default()
            .push(skill);
    }

    println!(
        "{} skill{} installed:\n",
        groups.len(),
        if groups.len() == 1 { "" } else { "s" }
    );

    for ((owner, name), entries) in &groups {
        let version = &entries[0].version;
        println!("  {owner}/{name} v{version}");
        for entry in entries {
            println!(
                "    -> {}  ({})",
                entry.installed_to.display(),
                entry.installed_at,
            );
        }
        println!();
    }

    ExitCode::SUCCESS
}
