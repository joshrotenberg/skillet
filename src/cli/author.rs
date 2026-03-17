use std::process::ExitCode;

use skillet_mcp::scaffold;

use crate::InitArgs;

/// Run the `init` subcommand.
pub(crate) fn run_init(args: InitArgs) -> ExitCode {
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

    let opts = scaffold::InitOptions {
        name: &name,
        description: args.description.as_deref(),
        include_skill: args.skill,
        include_multi: args.multi,
    };

    if let Err(e) = scaffold::init(path, &opts) {
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
