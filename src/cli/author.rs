use std::process::ExitCode;

use skillet_mcp::{config, safety, scaffold, validate};

use super::print_safety_report;
use crate::{InitProjectArgs, ValidateArgs};

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
