mod init;

use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::{generate, Shell};
use std::io;

use init::{
    check_migration_warning, detect_backend, install_service, list_corky_services,
    resolve_service, run_service_action, run_service_disable, run_service_enable,
    run_service_logs, uninstall_service, ServiceName,
    C_BOLD, C_RESET,
};

// ─────────────────────────────────────────────────────────────────────────────
// ASCII art banner for CLI help output
// ─────────────────────────────────────────────────────────────────────────────
const BANNER: &str = r#"
⠀⠀⠀⠀⠀⠀⢀⣀⣀⣀⣀⣀⡀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀
⠀⠀⠀⠀⠀⠺⢿⣿⣿⣿⣿⣿⣿⣷⣦⣠⣤⣤⣤⣄⣀⣀⠀⠀⠀⠀⠀⠀⠀⠀
⠀⠀⠀⠀⠀⠀⠀⠙⢿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣷⣦⣄⠀⠀⠀⠀
⠀⠀⠀⠀⠀⠀⢀⣴⣾⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⠿⠿⠿⣿⣿⣷⣄⠀⠀
⠀⠀⠀⠀⠀⢠⣾⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣀⠀⠀⠀⣀⣿⣿⣿⣆⠀
⠀⠀⠀⠀⢠⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⡄
⠀⠀⠀⠀⣾⣿⣿⡿⠋⠁⣀⣠⣬⣽⣿⣿⣿⣿⣿⣿⠿⠿⠿⠿⠿⠿⠿⠿⠟⠁
⠀⠀⠀⢀⣿⣿⡏⢀⣴⣿⠿⠛⠉⠉⠀⢸⣿⣿⠿⠁⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀
⠀⠀⠀⢸⣿⣿⢠⣾⡟⠁⠀⠀⠀⠀⠀⠈⠉⠁⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀
⠀⠀⠀⢸⣿⣿⣾⠏⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀
⠀⠀⠀⣸⣿⣿⣿⣀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀
⠀⢠⣾⣿⣿⣿⣿⣿⣷⣄⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀
⠀⣾⣿⣿⣿⣿⣿⣿⣿⣿⣦⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀
⢰⣿⡿⠛⠉⠀⠀⠀⠈⠙⠛⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀
⠈⠁⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀
"#;

// ─────────────────────────────────────────────────────────────────────────────
// CLI
// ─────────────────────────────────────────────────────────────────────────────
#[derive(Parser)]
#[command(name = "corky", version = "0.1.0", author = "Your Name", about = "Corky CLI manager")]
#[command(before_help = BANNER)]
#[command(arg_required_else_help = true)]
#[command(propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Install corky services (auto-detects init system)
    Install {
        /// Run in dry-run mode (no actual changes made)
        #[arg(long)]
        dry_run: bool,

        /// Skip init system registration (install binary and config only)
        #[arg(long, alias = "skip-service")]
        skip_init: bool,
    },
    /// Uninstall corky services
    Uninstall {
        /// Run in dry-run mode (no actual changes made)
        #[arg(long)]
        dry_run: bool,

        /// Skip init system operations (remove binary and config only)
        #[arg(long, alias = "skip-service")]
        skip_init: bool,
    },
    /// View logs for a corky service
    Logs {
        /// Name of the service to view logs for
        service: Option<ServiceName>,
    },
    /// Check status of a corky service
    Status {
        /// Name of the service to check status for
        service: Option<ServiceName>,
    },
    /// Start a corky service
    Start {
        /// Name of the service to start
        service: Option<ServiceName>,
    },
    /// Stop a corky service
    Stop {
        /// Name of the service to stop
        service: Option<ServiceName>,
    },
    /// Restart a corky service
    Restart {
        /// Name of the service to restart
        service: Option<ServiceName>,
    },
    /// Enable a corky service (auto-start)
    Enable {
        /// Name of the service to enable
        service: Option<ServiceName>,
    },
    /// Disable a corky service (no auto-start)
    Disable {
        /// Name of the service to disable
        service: Option<ServiceName>,
    },
    /// List all corky services
    List,
    /// Generate shell completion scripts
    Completion {
        /// Shell to generate completions for
        shell: Shell,
    },
    /// Print tab completion for shell - used internally by completion scripts
    #[command(hide = true)]
    CompletionItems,
}

// ─────────────────────────────────────────────────────────────────────────────
// main
// ─────────────────────────────────────────────────────────────────────────────
fn main() {
    let cli = Cli::parse();

    // Completion commands don't need backend detection (avoids errors in dev containers)
    match &cli.command {
        Commands::Completion { shell } => {
            generate_completion(*shell);
            return;
        }
        _ => {}
    }

    // Detect init system once, use everywhere
    let backend = detect_backend();

    // Warn about orphaned configs from a different backend
    check_migration_warning(&backend);

    match &cli.command {
        Commands::Install { dry_run, skip_init } => {
            install_service(&backend, *dry_run, *skip_init);
        }
        Commands::Uninstall { dry_run, skip_init } => {
            uninstall_service(&backend, *dry_run, *skip_init);
        }
        Commands::Logs { service } => {
            let info = resolve_service(&backend, service.clone());
            run_service_logs(&info);
        }
        Commands::Status { service } => {
            let info = resolve_service(&backend, service.clone());
            run_service_action("status", &info);
        }
        Commands::Start { service } => {
            let info = resolve_service(&backend, service.clone());
            run_service_action("start", &info);
        }
        Commands::Stop { service } => {
            let info = resolve_service(&backend, service.clone());
            run_service_action("stop", &info);
        }
        Commands::Restart { service } => {
            let info = resolve_service(&backend, service.clone());
            run_service_action("restart", &info);
        }
        Commands::Enable { service } => {
            let info = resolve_service(&backend, service.clone());
            run_service_enable(&info);
        }
        Commands::Disable { service } => {
            let info = resolve_service(&backend, service.clone());
            run_service_disable(&info);
        }
        Commands::List => {
            let services = list_corky_services(&backend);
            println!(
                "{}Available Corky Services [{}]:{}\n{}",
                C_BOLD,
                backend,
                C_RESET,
                "-".repeat(40)
            );
            if services.is_empty() {
                println!("  (none found)");
            }
            for s in &services {
                println!("  {} ({})", s.name, s.backend.display_label());
            }
        }
        Commands::Completion { .. } => unreachable!(), // handled above
        Commands::CompletionItems => {
            for s in list_corky_services(&backend) {
                println!("{}", s.name.replace("corky-", ""));
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Shell completion
// ─────────────────────────────────────────────────────────────────────────────
fn generate_completion(shell: Shell) {
    let mut cmd = Cli::command();
    let bin_name = cmd.get_name().to_string();

    generate(shell, &mut cmd, bin_name, &mut io::stdout());

    eprintln!("\nTo use these completions:");
    match shell {
        Shell::Bash => {
            eprintln!("Add the above to ~/.bash_completion or source it from your ~/.bashrc");
            eprintln!("Example: corky completion bash > ~/.bash_completion.d/corky");
        }
        Shell::Zsh => {
            eprintln!("Save the above to _corky in your fpath directory");
            eprintln!("Example: corky completion zsh > ~/.zsh/completions/_corky");
        }
        Shell::Fish => {
            eprintln!("Save the above to ~/.config/fish/completions/corky.fish");
            eprintln!("Example: corky completion fish > ~/.config/fish/completions/corky.fish");
        }
        _ => {
            eprintln!("Follow your shell's documentation for installing completion scripts");
        }
    }
}
