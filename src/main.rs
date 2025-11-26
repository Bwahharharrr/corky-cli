use clap::{CommandFactory, Parser, Subcommand, ValueEnum};
use clap_complete::{generate, Generator, Shell};
use std::env;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};

// ASCII art banner for CLI help output
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
    /// Install corky services
    Install {
        /// Run in dry-run mode (no actual changes made)
        #[arg(long)]
        dry_run: bool,
    },
    /// Uninstall corky services
    Uninstall {
        /// Run in dry-run mode (no actual changes made)
        #[arg(long)]
        dry_run: bool,
    },
    /// View logs for a corky service
    Logs {
        /// Name of the service to view logs for
        #[arg(value_enum)]
        service: Option<ServiceName>,
    },
    /// Check status of a corky service
    Status {
        /// Name of the service to check status for
        #[arg(value_enum)]
        service: Option<ServiceName>,
    },
    /// Start a corky service
    Start {
        /// Name of the service to start
        #[arg(value_enum)]
        service: Option<ServiceName>,
    },
    /// Stop a corky service
    Stop {
        /// Name of the service to stop
        #[arg(value_enum)]
        service: Option<ServiceName>,
    },
    /// Restart a corky service
    Restart {
        /// Name of the service to restart
        #[arg(value_enum)]
        service: Option<ServiceName>,
    },
    /// Enable a corky service
    Enable {
        /// Name of the service to enable
        #[arg(value_enum)]
        service: Option<ServiceName>,
    },
    /// Disable a corky service
    Disable {
        /// Name of the service to disable
        #[arg(value_enum)]
        service: Option<ServiceName>,
    },
    /// List all corky services
    List,
    /// Generate shell completion scripts
    Completion {
        /// Shell to generate completions for
        #[arg(value_enum)]
        shell: Shell,
    },
    /// Print tab completion for shell - used internally by completion scripts
    #[command(hide = true)]
    CompletionItems {
        /// Command to complete (logs, status, etc.)
        cmd: String,
    },
}

/// Service name specification - can be a special value or a custom service name
#[derive(Debug, Clone)]
enum ServiceName {
    /// Automatically select the service if only one is available
    Auto,
    /// Show information for all services
    All,
    /// Interactively select a service
    Interactive,
    /// A specific service name (can be full name or partial)
    Custom(String),
}

// Custom string parsing for ServiceName
impl std::str::FromStr for ServiceName {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "auto" => Ok(ServiceName::Auto),
            "all" => Ok(ServiceName::All),
            "interactive" => Ok(ServiceName::Interactive),
            _ => Ok(ServiceName::Custom(s.to_string())),
        }
    }
}

// Display implementation for ServiceName
impl std::fmt::Display for ServiceName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ServiceName::Auto => write!(f, "auto"),
            ServiceName::All => write!(f, "all"),
            ServiceName::Interactive => write!(f, "interactive"),
            ServiceName::Custom(s) => write!(f, "{}", s),
        }
    }
}

/// Check if the current process is running with root privileges
fn is_root() -> bool {
    #[cfg(unix)]
    {
        unsafe { libc::geteuid() == 0 }
    }
    #[cfg(not(unix))]
    {
        false // On non-unix platforms, assume not root
    }
}

/// Restart the current process with elevated privileges
fn elevate_privileges(args: &[String]) -> ! {
    println!("{}", "This operation requires root privileges.");
    
    // Construct the command to run with sudo
    let status = Command::new("sudo")
        .arg(env::current_exe().expect("Failed to get current executable path"))
        .args(args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .expect("Failed to execute sudo command");
    
    std::process::exit(status.code().unwrap_or(1));
}

/// Determine if the given command requires root privileges
fn requires_root(command: &Commands) -> bool {
    match command {
        Commands::Install { .. } => true,
        Commands::Uninstall { .. } => true,
        Commands::Start { service } => {
            // Only require root for system services
            if let Some(service_name) = service {
                if matches!(service_name, ServiceName::All) {
                    return true; // All includes system services
                }
            }
            // We'll check more specifically in run_systemctl
            false
        },
        Commands::Stop { service } => {
            if let Some(service_name) = service {
                if matches!(service_name, ServiceName::All) {
                    return true;
                }
            }
            false
        },
        Commands::Restart { service } => {
            if let Some(service_name) = service {
                if matches!(service_name, ServiceName::All) {
                    return true;
                }
            }
            false
        },
        Commands::Enable { service } => {
            if let Some(service_name) = service {
                if matches!(service_name, ServiceName::All) {
                    return true;
                }
            }
            false
        },
        Commands::Disable { service } => {
            if let Some(service_name) = service {
                if matches!(service_name, ServiceName::All) {
                    return true;
                }
            }
            false
        },
        // These commands don't modify system state, so they don't need root
        Commands::Logs { .. } => false,
        Commands::Status { .. } => false,
        Commands::List => false,
        Commands::Completion { .. } => false,
        Commands::CompletionItems { .. } => false,
    }
}

fn main() {
    let cli = Cli::parse();
    
    // Check if we need root privileges for this command
    if requires_root(&cli.command) && !is_root() {
        // Get the original command line arguments
        let args: Vec<String> = env::args().collect();
        
        // Re-execute with sudo
        elevate_privileges(&args[1..]);
    }
    
    match &cli.command {
        Commands::Install { dry_run } => run_embedded_script("install", *dry_run),
        Commands::Uninstall { dry_run } => run_embedded_script("uninstall", *dry_run),
        Commands::Logs { service } => {
            let service_info = resolve_service(service.clone());
            run_systemctl_logs(&service_info);
        }
        Commands::Status { service } => {
            let service_info = resolve_service(service.clone());
            run_systemctl("status", &service_info);
        }
        Commands::Start { service } => {
            let service_info = resolve_service(service.clone());
            run_systemctl("start", &service_info);
        }
        Commands::Stop { service } => {
            let service_info = resolve_service(service.clone());
            run_systemctl("stop", &service_info);
        }
        Commands::Restart { service } => {
            let service_info = resolve_service(service.clone());
            run_systemctl("restart", &service_info);
        }
        Commands::Enable { service } => {
            let service_info = resolve_service(service.clone());
            run_systemctl("enable", &service_info);
        }
        Commands::Disable { service } => {
            let service_info = resolve_service(service.clone());
            run_systemctl("disable", &service_info);
        }
        Commands::List => {
            println!("Available Corky Services:");
            println!("-------------------------");
            for (service, scope) in list_corky_services() {
                println!("  {} ({})", service, scope);
            }
        }
        Commands::Completion { shell } => {
            generate_completion(*shell);
        }
        Commands::CompletionItems { cmd: _ } => {
            // List available service names for tab completion
            for (service, _) in list_corky_services() {
                // Strip 'corky-' prefix for better user experience
                let service_name = service.replace("corky-", "");
                println!("{}", service_name);
            }
        }
    }
}

/// Embedded install and uninstall scripts
const INSTALL_SCRIPT: &str = include_str!("../cmds/install.sh");
const UNINSTALL_SCRIPT: &str = include_str!("../cmds/uninstall.sh");

/// Writes embedded script to a temp file and executes it from PWD
fn run_embedded_script(name: &str, dry_run: bool) {
    // Check for root privileges for install/uninstall operations
    if !is_root() {
        println!("This operation requires root privileges.");
        
        // Get the original command line arguments
        let args: Vec<String> = env::args().collect();
        
        // Re-execute with sudo
        elevate_privileges(&args[1..]);
    }
    
    // Check for Cargo.toml before executing
    let cargo_path = std::path::Path::new("Cargo.toml");
    if !cargo_path.exists() {
        eprintln!("Error: No Cargo.toml found in the current directory.");
        std::process::exit(1);
    }
    
    // Validate that the Cargo.toml has [corky] section with is_corky_package = true
    if name == "install" {
        let cargo_content = fs::read_to_string(cargo_path)
            .expect("Failed to read Cargo.toml");
            
        // Check for [corky] section with is_corky_package = true
        let has_corky_section = cargo_content.contains("[corky]")
            && cargo_content.contains("is_corky_package = true");
            
        if !has_corky_section {
            eprintln!("\x1b[1;31mError: This does not appear to be a Corky package.\x1b[0m");
            eprintln!("Only Corky packages can be installed with this script.");
            eprintln!("A Corky package must have [corky] section with is_corky_package = true in Cargo.toml.");
            std::process::exit(1);
        }
    }
    
    let script = match name {
        "install" => INSTALL_SCRIPT,
        "uninstall" => UNINSTALL_SCRIPT,
        _ => {
            eprintln!("Unknown script: {}", name);
            std::process::exit(1);
        }
    };
    
    // Create a temporary file for the script
    let temp_dir = std::env::temp_dir();
    let temp_file_path = temp_dir.join(format!("corky_{}_script.sh", name));
    
    // Write the script to the temp file
    let mut file = File::create(&temp_file_path).expect("Failed to create temp file");
    file.write_all(script.as_bytes()).expect("Failed to write script to temp file");
    
    // Make the script executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let metadata = fs::metadata(&temp_file_path).expect("Failed to get file metadata");
        let mut perms = metadata.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&temp_file_path, perms).expect("Failed to set file permissions");
    }
    
    // Build the command - use bash to execute the script instead of executing directly
    // This avoids the "Text file busy" error that can occur when trying to execute
    // a file that was just written
    let mut cmd = Command::new("bash");
    cmd.arg(&temp_file_path);
    
    // Add dry-run flag if specified
    if dry_run {
        cmd.arg("--dry-run");
    }
    
    // Execute the script through bash
    let status = cmd
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .expect("Failed to execute script");
    
    // Cleanup the temp file
    fs::remove_file(&temp_file_path).ok();
    
    // Exit with the same status as the script
    std::process::exit(status.code().unwrap_or(1));
}

fn list_corky_services() -> Vec<(String, String)> {
    let mut services = Vec::new();
    
    // Check for user services
    if let Ok(output) = Command::new("systemctl")
        .args(["--user", "list-unit-files", "corky-*.service", "--no-legend"])
        .output() {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                if let Some(service_info) = parse_corky_service(line, "user") {
                    services.push(service_info);
                }
            }
        }
    }
    
    // Check for system services
    if let Ok(output) = Command::new("systemctl")
        .args(["list-unit-files", "corky-*.service", "--no-legend"])
        .output() {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                if let Some(service_info) = parse_corky_service(line, "system") {
                    services.push(service_info);
                }
            }
        }
    }
    
    services
}

/// Parse corky-* services and label with "user" or "system"
fn parse_corky_service(line: &str, scope: &str) -> Option<(String, String)> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if !parts.is_empty() {
        let first_part = parts[0];
        if first_part.starts_with("corky-") && first_part.ends_with(".service") {
            let service_name = first_part.trim_end_matches(".service").to_string();
            return Some((service_name, scope.to_string()));
        }
    }
    None
}

/// Resolve a service name from the provided ServiceName enum value
/// This handles auto-selection, interactive selection, etc.
// Service with its scope (system or user)
struct ServiceInfo {
    name: String,
    scope: String,
}

fn resolve_service(arg: Option<ServiceName>) -> ServiceInfo {
    let services = list_corky_services();
    
    if services.is_empty() {
        eprintln!("No Corky services found. You may need to install a service first.");
        std::process::exit(1);
    }
    
    let service_name = match arg {
        Some(ServiceName::Auto) => {
            if services.len() == 1 {
                // Only one service, return it
                services[0].0.clone()
            } else {
                // Multiple services, need to prompt
                eprintln!("Multiple services found. Please specify one:");
                for (i, (service, scope)) in services.iter().enumerate() {
                    eprintln!("  {}. {} ({})", i + 1, service, scope);
                }
                std::process::exit(1);
            }
        },
        Some(ServiceName::All) => {
            eprintln!("Cannot perform this operation on all services.");
            eprintln!("Please specify a single service name.");
            std::process::exit(1);
        },
        Some(ServiceName::Interactive) => {
            // Use inquire crate for interactive selection
            if services.is_empty() {
                eprintln!("No services found to select from.");
                std::process::exit(1);
            }
            
            // Format options with scope
            let options: Vec<String> = services
                .iter()
                .map(|(name, scope)| {
                    let display_name = name.replace("corky-", "");
                    format!("{} ({})", display_name, scope)
                })
                .collect();
            
            // Clone options before moving them into the prompt
            match inquire::Select::new("Select a service:", options.clone()).prompt() {
                Ok(selected) => {
                    // Extract the service name from the selection (remove the scope part)
                    let index = options.iter().position(|o| o == &selected).unwrap();
                    services[index].0.clone()
                },
                Err(_) => {
                    eprintln!("Service selection cancelled.");
                    std::process::exit(1);
                }
            }
        },
        Some(ServiceName::Custom(name)) => {
            // Check if this is an abbreviated name (e.g., "telegram" instead of "corky-telegram")
            let name_with_prefix = if !name.starts_with("corky-") {
                format!("corky-{}", name)
            } else {
                name
            };
            
            // Find matching service
            let matches: Vec<_> = services
                .iter()
                .filter(|(service, _)| service == &name_with_prefix)
                .collect();
            
            if matches.is_empty() {
                eprintln!("No service found with name: {}", name_with_prefix);
                eprintln!("Available services:");
                for (service, scope) in &services {
                    eprintln!("  {} ({})", service, scope);
                }
                std::process::exit(1);
            } else if matches.len() > 1 {
                eprintln!("Multiple services match the name: {}", name_with_prefix);
                eprintln!("Please specify which one:");
                for (i, (service, scope)) in matches.iter().enumerate() {
                    eprintln!("  {}. {} ({})", i + 1, service, scope);
                }
                std::process::exit(1);
            } else {
                matches[0].0.clone()
            }
        },
        None => {
            // Default to interactive selection
            if services.len() == 1 {
                // Only one service, return it
                services[0].0.clone()
            } else {
                // Use inquire crate for interactive selection
                // Format options with scope
                let options: Vec<String> = services
                    .iter()
                    .map(|(name, scope)| {
                        let display_name = name.replace("corky-", "");
                        format!("{} ({})", display_name, scope)
                    })
                    .collect();
                
                // Clone options before moving them into the prompt
                match inquire::Select::new("Select a service:", options.clone()).prompt() {
                    Ok(selected) => {
                        // Extract the service name from the selection
                        let index = options.iter().position(|o| o == &selected).unwrap();
                        services[index].0.clone()
                    },
                    Err(_) => {
                        eprintln!("Service selection cancelled.");
                        std::process::exit(1);
                    }
                }
            }
        }
    };
    
    // Find the scope for the selected service
    let scope = services
        .iter()
        .find(|(name, _)| name == &service_name)
        .map(|(_, scope)| scope.clone())
        .unwrap_or_else(|| "system".to_string());
    
    ServiceInfo {
        name: service_name,
        scope,
    }
}

fn run_systemctl(action: &str, service_info: &ServiceInfo) {
    // Check if we need root privileges for system services
    if service_info.scope == "system" && !is_root() {
        println!("This operation requires root privileges to manage system services.");
        
        // Get the original command line arguments
        let args: Vec<String> = env::args().collect();
        
        // Re-execute with sudo
        elevate_privileges(&args[1..]);
    }
    
    let mut cmd = Command::new("systemctl");
    
    // Add --user flag for user services
    if service_info.scope == "user" {
        cmd.arg("--user");
    }
    
    println!("Running: systemctl {} {}.service", 
             action,
             service_info.name);

    // For status command, we use inherit to show the rich formatted output
    if action == "status" {
        let status = cmd
            .arg(action)
            .arg(format!("{}.service", service_info.name))
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .expect("Failed to run systemctl");
            
        std::process::exit(status.code().unwrap_or(1));
    } else {
        // For other commands, capture output and display it
        match cmd
            .arg(action)
            .arg(format!("{}.service", service_info.name))
            .output() {
                Ok(output) => {
                    let exit_code = output.status.code().unwrap_or(1);
                    
                    // Display stdout if not empty
                    if !output.stdout.is_empty() {
                        println!("{}", String::from_utf8_lossy(&output.stdout));
                    }
                    
                    // Display stderr if not empty
                    if !output.stderr.is_empty() {
                        eprintln!("{}", String::from_utf8_lossy(&output.stderr));
                    }
                    
                    // Provide feedback based on exit code
                    if exit_code == 0 {
                        let action_past = match action {
                            "start" => "started",
                            "stop" => "stopped",
                            "restart" => "restarted",
                            "enable" => "enabled",
                            "disable" => "disabled",
                            _ => "processed"
                        };
                        println!("Service {} successfully {}.", service_info.name, action_past);
                    } else {
                        eprintln!("Failed to {} service {}. Exit code: {}", 
                                 action, service_info.name, exit_code);
                    }
                    
                    std::process::exit(exit_code);
                },
                Err(e) => {
                    eprintln!("Failed to execute systemctl: {}", e);
                    std::process::exit(1);
                }
            }
    }
}

fn run_systemctl_logs(service_info: &ServiceInfo) {
    // Check if we need root privileges for system services
    if service_info.scope == "system" && !is_root() {
        println!("This operation requires root privileges to view system service logs.");
        
        // Get the original command line arguments
        let args: Vec<String> = env::args().collect();
        
        // Re-execute with sudo
        elevate_privileges(&args[1..]);
    }
    
    let mut cmd = Command::new("journalctl");
    
    // Add --user flag for user services
    if service_info.scope == "user" {
        cmd.arg("--user");
    }
    
    let status = cmd
        .arg("-u")
        .arg(format!("{}.service", service_info.name))
        .arg("-f")
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .expect("Failed to run journalctl");
        
    std::process::exit(status.code().unwrap_or(1));
}

/// Generate shell completion script for corky
fn generate_completion(shell: Shell) {
    let mut cmd = Cli::command();
    let bin_name = cmd.get_name().to_string();

    // Print completion script to stdout
    generate(shell, &mut cmd, bin_name, &mut io::stdout());

    // Print installation instructions
    eprintln!("\nTo use these completions:");
    match shell {
        Shell::Bash => {
            eprintln!("Add the above to ~/.bash_completion or source it from your ~/.bashrc");
            eprintln!("Example: corky completion bash > ~/.bash_completion.d/corky");
        },
        Shell::Zsh => {
            eprintln!("Save the above to _corky in your fpath directory");
            eprintln!("Example: corky completion zsh > ~/.zsh/completions/_corky");
        },
        Shell::Fish => {
            eprintln!("Save the above to ~/.config/fish/completions/corky.fish");
            eprintln!("Example: corky completion fish > ~/.config/fish/completions/corky.fish");
        },
        _ => {
            eprintln!("Follow your shell's documentation for installing completion scripts");
        },
    }
}
