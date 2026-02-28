use std::process::ExitCode;

use skillet_mcp::{config, pack, publish, registry, safety, scaffold, validate};

use super::print_safety_report;
use crate::{
    InitProjectArgs, InitRegistryArgs, InitSkillArgs, PackArgs, PublishArgs, ValidateArgs,
};

/// Run the `validate` subcommand.
pub(crate) fn run_validate(args: ValidateArgs) -> ExitCode {
    let path = &args.path;
    println!("Validating {} ...\n", path.display());

    let result = match validate::validate_skillpack(path) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("  error: {e}");
            eprintln!("\nValidation failed.");
            return ExitCode::from(1);
        }
    };

    // skill.toml
    println!("  skill.toml ............ ok");

    // SKILL.md
    let line_count = result.skill_md.lines().count();
    println!("  SKILL.md .............. ok ({line_count} lines)");

    // Core fields
    println!("  owner ................. {}", result.owner);
    println!("  name .................. {}", result.name);
    println!("  version ............... {}", result.version);
    println!("  description ........... {}", result.description);

    // Categories and tags
    if let Some(ref classification) = result.metadata.skill.classification {
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

    // Extra files
    if !result.files.is_empty() {
        let mut file_paths: Vec<&String> = result.files.keys().collect();
        file_paths.sort();
        println!(
            "  extra files ........... {}",
            file_paths
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }

    // Content hash (show abbreviated)
    let hash_display = if result.hashes.composite.len() > 17 {
        format!("{}...", &result.hashes.composite[..17])
    } else {
        result.hashes.composite.clone()
    };
    println!("  content hash .......... {hash_display}");

    // Manifest status
    match result.manifest_ok {
        Some(true) => println!("  manifest .............. verified"),
        Some(false) => println!("  manifest .............. MISMATCH"),
        None => println!("  manifest .............. not found (will be generated on publish)"),
    }

    // Warnings
    if !result.warnings.is_empty() {
        println!();
        for w in &result.warnings {
            println!("  warning: {w}");
        }
    }

    // Safety scanning
    if !args.skip_safety {
        let cli_config = config::load_config().unwrap_or_default();
        let report = safety::scan(
            &result.skill_md,
            &result.skill_toml_raw,
            &result.files,
            &result.metadata,
            &cli_config.safety.suppress,
        );

        if !report.is_empty() {
            println!();
            print_safety_report(&report);
        }

        if report.has_danger() {
            eprintln!("\nValidation failed: safety issues detected.");
            return ExitCode::from(2);
        }
    }

    println!("\nValidation passed.");
    ExitCode::SUCCESS
}

/// Run the `pack` subcommand.
pub(crate) fn run_pack(args: PackArgs) -> ExitCode {
    let path = &args.path;
    println!("Packing {} ...\n", path.display());

    let result = match pack::pack(path) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("  error: {e}");
            eprintln!("\nPack failed.");
            return ExitCode::from(1);
        }
    };

    let v = &result.validation;
    println!("  owner ................. {}", v.owner);
    println!("  name .................. {}", v.name);
    println!("  version ............... {}", v.version);

    if result.manifest_written {
        println!("  MANIFEST.sha256 ....... written");
    }

    if result.versions_updated {
        println!("  versions.toml ......... updated");
    } else {
        println!("  versions.toml ......... up to date");
    }

    // Safety scanning
    if !args.skip_safety {
        let cli_config = config::load_config().unwrap_or_default();
        let report = safety::scan(
            &v.skill_md,
            &v.skill_toml_raw,
            &v.files,
            &v.metadata,
            &cli_config.safety.suppress,
        );

        if !report.is_empty() {
            println!();
            print_safety_report(&report);
        }

        if report.has_danger() {
            eprintln!("\nPack failed: safety issues detected.");
            return ExitCode::from(2);
        }
    }

    println!("\nPack succeeded.");
    ExitCode::SUCCESS
}

/// Run the `publish` subcommand.
pub(crate) fn run_publish(args: PublishArgs) -> ExitCode {
    let path = &args.path;
    println!("Publishing {} to {} ...\n", path.display(), args.repo);

    let result = match publish::publish(
        path,
        &args.repo,
        args.registry_path.as_deref(),
        args.dry_run,
    ) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("  error: {e}");
            eprintln!("\nPublish failed.");
            return ExitCode::from(1);
        }
    };

    let v = &result.pack.validation;
    println!("  owner ................. {}", v.owner);
    println!("  name .................. {}", v.name);
    println!("  version ............... {}", v.version);

    // Safety scanning
    if !args.skip_safety {
        let cli_config = config::load_config().unwrap_or_default();
        let report = safety::scan(
            &v.skill_md,
            &v.skill_toml_raw,
            &v.files,
            &v.metadata,
            &cli_config.safety.suppress,
        );

        if !report.is_empty() {
            println!();
            print_safety_report(&report);
        }

        if report.has_danger() {
            eprintln!("\nPublish failed: safety issues detected.");
            return ExitCode::from(2);
        }
    }

    if args.dry_run {
        println!("\nDry run complete.");
    } else {
        println!("  PR .................... {}", result.pr_url);
        println!("\nPublish succeeded.");
    }

    ExitCode::SUCCESS
}

/// Run the `init-registry` subcommand.
pub(crate) fn run_init_registry(args: InitRegistryArgs) -> ExitCode {
    let path = &args.path;

    if path.exists() {
        eprintln!("Error: {} already exists", path.display());
        return ExitCode::from(1);
    }

    let name = args
        .name
        .or_else(|| {
            path.file_name()
                .and_then(|n| n.to_str())
                .map(|s| s.to_string())
        })
        .unwrap_or_else(|| "my-skills".to_string());

    if let Err(e) = registry::init_registry(path, &name, args.description.as_deref()) {
        eprintln!("Error: {e}");
        return ExitCode::from(1);
    }

    println!("Initialized skill registry at {}", path.display());
    println!();
    println!("  cd {}", path.display());
    println!("  # add skills: mkdir -p owner/skill-name");
    println!("  # serve locally: skillet --registry .");
    println!("  # push and serve remotely: skillet --remote <git-url>");

    ExitCode::SUCCESS
}

/// Run the `init-skill` subcommand.
pub(crate) fn run_init_skill(args: InitSkillArgs) -> ExitCode {
    let path = &args.path;

    // Infer owner/name from path components
    let name = match path.file_name().and_then(|n| n.to_str()) {
        Some(n) if !n.is_empty() => n.to_string(),
        _ => {
            eprintln!("Error: could not infer skill name from path");
            return ExitCode::from(1);
        }
    };

    let owner = match path
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
    {
        Some(o) if !o.is_empty() => o.to_string(),
        _ => {
            eprintln!(
                "Error: could not infer owner from path. Use owner/skill-name format (e.g. myname/my-skill)"
            );
            return ExitCode::from(1);
        }
    };

    let description = args
        .description
        .unwrap_or_else(|| format!("A skill for {name}"));

    if let Err(e) = scaffold::init_skill(
        path,
        &owner,
        &name,
        &description,
        &args.category,
        &args.tags,
    ) {
        eprintln!("Error: {e}");
        return ExitCode::from(1);
    }

    println!("Created skillpack at {}", path.display());
    println!();
    println!("  owner ................. {owner}");
    println!("  name .................. {name}");
    println!();
    println!("Next steps:");
    println!(
        "  1. Edit {}/skill.toml to customize metadata",
        path.display()
    );
    println!(
        "  2. Edit {}/SKILL.md to write your skill prompt",
        path.display()
    );
    println!("  3. Validate: skillet validate {}", path.display());

    ExitCode::SUCCESS
}

/// Run the `init-project` subcommand.
pub(crate) fn run_init_project(args: InitProjectArgs) -> ExitCode {
    let path = &args.path;

    // Ensure directory exists (for "." it already does)
    if !path.exists()
        && let Err(e) = std::fs::create_dir_all(path)
    {
        eprintln!("Error creating directory: {e}");
        return ExitCode::from(1);
    }

    let name = args
        .name
        .or_else(|| {
            // Resolve "." to absolute path for name inference
            let resolved = if path == std::path::Path::new(".") {
                std::env::current_dir().ok()
            } else {
                Some(path.to_path_buf())
            };
            resolved.and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
        })
        .unwrap_or_else(|| "my-project".to_string());

    let opts = scaffold::InitProjectOptions {
        name: &name,
        description: args.description.as_deref(),
        include_skill: args.skill,
        include_multi: args.multi,
        include_registry: args.registry,
    };

    if let Err(e) = scaffold::init_project(path, &opts) {
        eprintln!("Error: {e}");
        return ExitCode::from(1);
    }

    println!("Created skillet.toml at {}", path.display());
    println!();
    println!("  project ............... {name}");
    if args.skill {
        println!("  [skill] ............... included");
    }
    if args.multi {
        println!("  [skills] .............. included (.skillet/)");
    }
    if args.registry {
        println!("  [registry] ............ included");
    }
    println!();
    println!("Next steps:");
    println!(
        "  1. Edit {}/skillet.toml to customize project metadata",
        path.display()
    );
    if args.skill {
        println!(
            "  2. Edit {}/SKILL.md to write your skill prompt",
            path.display()
        );
    }
    if args.multi {
        println!(
            "  2. Add skills to {}/.skillet/ (each needs a SKILL.md)",
            path.display()
        );
    }

    ExitCode::SUCCESS
}
