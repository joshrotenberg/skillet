use std::process::ExitCode;

use skillet_mcp::{config, registry};

use crate::SetupArgs;

/// Run the `setup` subcommand.
pub(crate) fn run_setup(args: SetupArgs) -> ExitCode {
    let config_path = config::config_dir().join("config.toml");

    // Check for existing config
    if config_path.exists() && !args.force {
        eprintln!(
            "Config already exists at {}\nUse --force to overwrite.",
            config_path.display()
        );
        return ExitCode::from(1);
    }

    // Build remotes list: official first (unless opted out), then user-provided
    let mut remotes = Vec::new();
    if !args.no_official_registry {
        remotes.push(registry::DEFAULT_REGISTRY_URL.to_string());
    }
    remotes.extend(args.remote);

    let config = match config::generate_default_config(remotes, args.registry, &args.target) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error: {e}");
            return ExitCode::from(1);
        }
    };

    let path = match config::write_config(&config) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Error writing config: {e}");
            return ExitCode::from(1);
        }
    };

    // Read back the written content to display
    let content = std::fs::read_to_string(&path).unwrap_or_default();

    println!("Wrote {}\n", path.display());
    println!("{content}");
    println!(
        "To use skillet with your agent, add this to your MCP config:\n\n\
         {{\n  \"mcpServers\": {{\n    \"skillet\": {{\n      \"command\": \"skillet\"\n    }}\n  }}\n}}"
    );
    println!(
        "\nNext steps:\n  \
         skillet search *            # browse available skills\n  \
         skillet info owner/name     # see skill details\n  \
         skillet install owner/name  # install a skill"
    );

    ExitCode::SUCCESS
}
