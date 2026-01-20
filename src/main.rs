use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::{generate, Shell};
use serde::Deserialize;
use std::env;
use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

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
// Constants & colors
// ─────────────────────────────────────────────────────────────────────────────
const SUDO_BIN: &str = "sudo";
const ENV_ELEVATED_FLAG: &str = "CORKY_ELEVATED"; // prevents re-entrancy loops
const ENV_BINARY_CHECKSUM: &str = "CORKY_BINARY_CHECKSUM"; // for TOCTOU protection
const ENV_ORIGINAL_CWD: &str = "CORKY_ORIGINAL_CWD"; // preserve cwd across sudo
const BIN_PATH_SYSTEM: &str = "/usr/local/bin";
const UNIT_DIR_SYSTEM: &str = "/etc/systemd/system";

const C_RESET: &str = "\x1b[0m";
const C_BOLD: &str = "\x1b[1m";
const C_GREEN: &str = "\x1b[32m";
const C_BGREEN: &str = "\x1b[1;32m";
const C_RED: &str = "\x1b[31m";
const C_YELLOW: &str = "\x1b[33m";
const C_BLUE: &str = "\x1b[34m";
const C_WHITE: &str = "\x1b[37m";
const C_CYAN: &str = "\x1b[36m";

// ─────────────────────────────────────────────────────────────────────────────
// TOML structures for Cargo.toml parsing
// ─────────────────────────────────────────────────────────────────────────────
#[derive(Deserialize)]
struct CargoToml {
    package: Option<Package>,
    corky: Option<CorkyConfig>,
}

#[derive(Deserialize)]
struct Package {
    name: Option<String>,
    description: Option<String>,
}

#[derive(Deserialize)]
struct CorkyConfig {
    is_corky_package: Option<bool>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Small helper functions
// ─────────────────────────────────────────────────────────────────────────────

/// Print an error message and exit with status 1.
fn exit_error(msg: &str) -> ! {
    eprintln!("{C_RED}[ERROR]{C_RESET} {}", msg);
    std::process::exit(1);
}

/// Ensure service name has the corky- prefix.
fn ensure_corky_prefix(name: &str) -> String {
    if name.starts_with("corky-") {
        name.to_string()
    } else {
        format!("corky-{}", name)
    }
}

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
    /// Install corky services (system service)
    Install {
        /// Run in dry-run mode (no actual changes made)
        #[arg(long)]
        dry_run: bool,
    },
    /// Uninstall corky services (system service)
    Uninstall {
        /// Run in dry-run mode (no actual changes made)
        #[arg(long)]
        dry_run: bool,
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
    /// Enable a corky service
    Enable {
        /// Name of the service to enable
        service: Option<ServiceName>,
    },
    /// Disable a corky service
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

/// Service name specification - can be a special value or a custom service name
#[derive(Debug, Clone)]
enum ServiceName {
    Auto,
    All,
    Interactive,
    Custom(String),
}

impl std::str::FromStr for ServiceName {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "auto" => Ok(ServiceName::Auto),
            "all" => Ok(ServiceName::All),
            "interactive" => Ok(ServiceName::Interactive),
            _ => {
                // Validate service name: only alphanumeric, underscore, and hyphen allowed
                if !is_valid_service_name(s) {
                    return Err(format!(
                        "Invalid service name '{}'. Only alphanumeric characters, underscores, and hyphens are allowed.",
                        s
                    ));
                }
                Ok(ServiceName::Custom(s.to_string()))
            }
        }
    }
}

/// Validate that a service name contains only safe characters.
/// Allowed: alphanumeric, underscore (_), hyphen (-)
fn is_valid_service_name(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

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

// ─────────────────────────────────────────────────────────────────────────────
// Privilege helpers
// ─────────────────────────────────────────────────────────────────────────────
fn is_root() -> bool {
    #[cfg(unix)]
    {
        unsafe { libc::geteuid() == 0 }
    }
    #[cfg(not(unix))]
    {
        false
    }
}

/// Prime sudo timestamp so the password is asked at most once per sudo session timeout.
fn ensure_sudo_timestamp() {
    if is_root() {
        return;
    }
    let _ = Command::new(SUDO_BIN)
        .arg("-v")
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status();
}

/// Restart the current process with elevated privileges (re-exec via sudo).
/// Uses an env flag to avoid recursion loops if the elevated run calls this again.
/// Accepts additional environment variables to pass through sudo.
fn elevate_privileges(args: &[String], extra_env: &[(&str, &str)]) -> ! {
    if env::var_os(ENV_ELEVATED_FLAG).is_some() {
        eprintln!("{C_RED}Elevation loop detected; aborting.{C_RESET}");
        std::process::exit(1);
    }
    ensure_sudo_timestamp();
    let exe = env::current_exe().expect("Failed to get current executable path");

    // Capture cwd before sudo changes it
    let cwd = env::current_dir().ok();
    let cwd_str = cwd.as_ref().map(|p| p.to_string_lossy().to_string());

    let mut cmd = Command::new(SUDO_BIN);
    cmd.env(ENV_ELEVATED_FLAG, "1");

    // Pass original working directory through env var
    if let Some(ref dir) = cwd_str {
        cmd.env(ENV_ORIGINAL_CWD, dir);
    }

    // Pass any additional environment variables
    for (key, value) in extra_env {
        cmd.env(*key, *value);
    }

    cmd.arg(exe)
        .args(args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    if let Some(dir) = cwd {
        let _ = cmd.current_dir(dir);
    }

    let status = cmd.status().expect("Failed to execute sudo re-exec");
    std::process::exit(status.code().unwrap_or(1));
}

// ─────────────────────────────────────────────────────────────────────────────
// main
// ─────────────────────────────────────────────────────────────────────────────
fn main() {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Install { dry_run } => install_system_service(*dry_run),
        Commands::Uninstall { dry_run } => uninstall_system_service(*dry_run),
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
            println!("{}Available Corky Services:{}\n-------------------------", C_BOLD, C_RESET);
            for s in list_corky_services() {
                println!("  {} ({})", s.name, s.scope);
            }
        }
        Commands::Completion { shell } => {
            generate_completion(*shell);
        }
        Commands::CompletionItems => {
            for s in list_corky_services() {
                println!("{}", s.name.replace("corky-", ""));
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// INSTALL / UNINSTALL (Pure Rust) — no empty sections
// ─────────────────────────────────────────────────────────────────────────────

fn install_system_service(dry_run: bool) {
    // Phase 1 (user): validate + build, then elevate. No empty "Elevation" section.
    if !is_root() {
        section("Validating Corky package");
        validate_corky_package_or_exit();
        println!("{C_BGREEN}[OK]{C_RESET} {C_WHITE}Found Cargo.toml with [corky] is_corky_package=true{C_RESET}");

        section("Building (release)");
        if dry_run {
            println!("{C_CYAN}[DRY-RUN]{C_RESET} Would run: cargo build --release");
        } else {
            println!("{C_GREEN}[INFO]{C_RESET} Running: cargo build --release");
            let status = Command::new("cargo")
                .arg("build")
                .arg("--release")
                .stdin(Stdio::inherit())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .status()
                .expect("Failed to run cargo build --release");
            if !status.success() {
                eprintln!("{C_RED}[ERROR]{C_RESET} cargo build --release failed.");
                std::process::exit(status.code().unwrap_or(1));
            }
        }

        // Compute checksum of built binary for TOCTOU protection
        let (raw_pkg_name, _) = pkg_name_and_description().unwrap_or_else(|| {
            ("corky".to_string(), "corky service".to_string())
        });
        let target_bin = Path::new("target").join("release").join(&raw_pkg_name);
        let checksum = if !dry_run {
            compute_file_checksum(&target_bin).unwrap_or_else(|| {
                eprintln!("{C_RED}[ERROR]{C_RESET} Failed to compute checksum of {}", target_bin.display());
                std::process::exit(1);
            })
        } else {
            String::new()
        };

        // Single, informative line instead of an empty heading
        println!("{C_GREEN}[INFO]{C_RESET} Elevating with sudo to install system service…");
        let args: Vec<String> = env::args().skip(1).collect();
        elevate_privileges(&args, &[(ENV_BINARY_CHECKSUM, &checksum)]);
    }

    // Phase 2 (root): install only — no duplicate validation/build sections
    // Use preserved working directory from before sudo elevation
    let cwd = env::var(ENV_ORIGINAL_CWD)
        .ok()
        .map(PathBuf::from)
        .unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| PathBuf::from("/")));

    // Change to the original directory if different
    if let Err(e) = env::set_current_dir(&cwd) {
        eprintln!("{C_YELLOW}[WARN]{C_RESET} Could not change to original directory {}: {}", cwd.display(), e);
    }

    let (raw_pkg_name, description) = pkg_name_and_description().unwrap_or_else(|| {
        eprintln!("{C_YELLOW}[WARN]{C_RESET} Using fallback metadata (could not read Cargo.toml).");
        ("corky".to_string(), "corky service".to_string())
    });

    // Ensure service name has corky- prefix for discovery by list_corky_services()
    let service_name = ensure_corky_prefix(&raw_pkg_name);

    let target_bin = Path::new("target").join("release").join(&raw_pkg_name);
    let install_bin = Path::new(BIN_PATH_SYSTEM).join(&raw_pkg_name);
    let unit_path = Path::new(UNIT_DIR_SYSTEM).join(format!("{}.service", &service_name));

    // Verify binary integrity (TOCTOU protection)
    if !dry_run {
        if let Ok(expected_checksum) = env::var(ENV_BINARY_CHECKSUM) {
            if !expected_checksum.is_empty() {
                let actual_checksum = compute_file_checksum(&target_bin).unwrap_or_default();
                if actual_checksum != expected_checksum {
                    eprintln!("{C_RED}[ERROR]{C_RESET} Binary checksum mismatch! The binary may have been tampered with.");
                    eprintln!("Expected: {}", expected_checksum);
                    eprintln!("Actual:   {}", actual_checksum);
                    eprintln!("Please rebuild and try again.");
                    std::process::exit(1);
                }
                println!("{C_GREEN}[INFO]{C_RESET} Binary integrity verified.");
            }
        }
    }

    section("Installing binary");
    if dry_run {
        println!(
            "{C_CYAN}[DRY-RUN]{C_RESET} Would install: {} -> {} (0755)",
            target_bin.display(),
            install_bin.display()
        );
    } else {
        let data = fs::read(&target_bin).unwrap_or_else(|e| {
            eprintln!("{C_RED}[ERROR]{C_RESET} Read {}: {}", target_bin.display(), e);
            std::process::exit(1);
        });
        fs::create_dir_all(BIN_PATH_SYSTEM).unwrap_or_else(|e| {
            eprintln!("{C_RED}[ERROR]{C_RESET} create {}: {}", BIN_PATH_SYSTEM, e);
            std::process::exit(1);
        });
        fs::write(&install_bin, &data).unwrap_or_else(|e| {
            eprintln!("{C_RED}[ERROR]{C_RESET} write {}: {}", install_bin.display(), e);
            std::process::exit(1);
        });
        let mut perms = fs::metadata(&install_bin)
            .map(|m| m.permissions())
            .unwrap_or_else(|e| {
                eprintln!("{C_RED}[ERROR]{C_RESET} stat {}: {}", install_bin.display(), e);
                std::process::exit(1);
            });
        perms.set_mode(0o755);
        fs::set_permissions(&install_bin, perms).unwrap_or_else(|e| {
            eprintln!("{C_RED}[ERROR]{C_RESET} chmod {}: {}", install_bin.display(), e);
            std::process::exit(1);
        });
        println!("{C_GREEN}[INFO]{C_RESET} Installed: {}", install_bin.display());
    }

    section("Writing systemd unit");
    if !cwd.is_dir() {
        eprintln!("{C_RED}[ERROR]{C_RESET} Working directory does not exist: {}", cwd.display());
        eprintln!("Please run this command from a valid directory.");
        std::process::exit(1);
    }
    let user = installing_user();
    let group = primary_group_for_user(&user);
    let unit_contents = format!(
        r#"[Unit]
Description={description}
Wants=network-online.target
After=network-online.target

[Service]
User={user}
Group={group}
WorkingDirectory={workdir}
ExecStart={exec_path}
ExecStartPre=/usr/bin/test -x {exec_path}
Restart=on-failure
RestartSec=1
ProtectHome=no

[Install]
WantedBy=multi-user.target
"#,
        description = description,
        user = user,
        group = group,
        workdir = cwd.display(),
        exec_path = install_bin.display(),
    );

    if dry_run {
        println!("{C_CYAN}[DRY-RUN]{C_RESET} Would write unit: {}", unit_path.display());
        println!("---------- unit file ----------\n{}\n-------------------------------", unit_contents);
    } else {
        fs::write(&unit_path, unit_contents).unwrap_or_else(|e| {
            eprintln!("{C_RED}[ERROR]{C_RESET} write {}: {}", unit_path.display(), e);
            std::process::exit(1);
        });
        // Best-effort SELinux relabel
        let _ = Command::new("restorecon").arg("-v").arg(&unit_path).status();
        let _ = Command::new("restorecon").arg("-v").arg(&install_bin).status();
        println!("{C_GREEN}[INFO]{C_RESET} Wrote unit: {}", unit_path.display());
    }

    section("Reloading & enabling service");
    if dry_run {
        println!("{C_CYAN}[DRY-RUN]{C_RESET} Would run: systemctl daemon-reload");
        println!("{C_CYAN}[DRY-RUN]{C_RESET} Would run: systemctl enable {}", service_name);
        println!("{C_CYAN}[DRY-RUN]{C_RESET} Would run: systemctl start {}", service_name);
        return;
    }
    run_cmd_expect_ok("systemctl", &["daemon-reload"]);
    run_cmd_expect_ok("systemctl", &["enable", &service_name]);
    let ok = run_cmd("systemctl", &["start", &service_name]);
    if !ok {
        eprintln!("{C_RED}[ERROR]{C_RESET} Failed to start service. Try: systemctl status {}", service_name);
        std::process::exit(1);
    }

    section("Done");
    println!("{C_BGREEN}[SUCCESS]{C_RESET} {} installed and started.", service_name);
    println!("• Binary: {}", install_bin.display());
    println!("• Unit:   {}", unit_path.display());
    println!("→ Manage with: systemctl [status|restart|stop] {}", service_name);
}

fn uninstall_system_service(dry_run: bool) {
    // Phase 1 (user): elevate, no empty headings.
    if !is_root() {
        println!("{C_GREEN}[INFO]{C_RESET} Elevating with sudo to uninstall system service…");
        let args: Vec<String> = env::args().skip(1).collect();
        elevate_privileges(&args, &[]);
    }

    // Phase 2 (root)
    // Use preserved working directory from before sudo elevation
    if let Ok(original_cwd) = env::var(ENV_ORIGINAL_CWD) {
        let _ = env::set_current_dir(&original_cwd);
    }

    section("Uninstall (system service)");
    let (raw_pkg_name, _) =
        pkg_name_and_description().unwrap_or_else(|| ("corky".to_string(), "corky service".to_string()));

    // Ensure service name has corky- prefix (matching install behavior)
    let service_name = ensure_corky_prefix(&raw_pkg_name);

    let unit = format!("{}.service", &service_name);
    let unit_path = Path::new(UNIT_DIR_SYSTEM).join(&unit);
    let bin_path = Path::new(BIN_PATH_SYSTEM).join(&raw_pkg_name);

    // Stop & disable if present
    section("Stopping & disabling");
    if dry_run {
        println!("{C_CYAN}[DRY-RUN]{C_RESET} Would run: systemctl stop {}", service_name);
        println!("{C_CYAN}[DRY-RUN]{C_RESET} Would run: systemctl disable {}", service_name);
    } else {
        // best-effort; we keep stdout/stderr inherited so the user sees real actions (like symlink removal)
        let _ = run_cmd("systemctl", &["stop", &service_name]);
        let _ = run_cmd("systemctl", &["disable", &service_name]);
    }

    // Remove unit + reload; use quiet reset-failed so no confusing error text
    section("Removing unit & reloading daemon");
    if dry_run {
        println!("{C_CYAN}[DRY-RUN]{C_RESET} Would remove: {}", unit_path.display());
        println!("{C_CYAN}[DRY-RUN]{C_RESET} Would run: systemctl daemon-reload");
        println!("{C_CYAN}[DRY-RUN]{C_RESET} Would run: systemctl reset-failed {}", service_name);
    } else {
        if unit_path.exists() {
            println!("{C_GREEN}[INFO]{C_RESET} Removing unit {}", unit_path.display());
            if let Err(e) = fs::remove_file(&unit_path) {
                eprintln!("{C_YELLOW}[WARN]{C_RESET} remove {}: {}", unit_path.display(), e);
            }
        } else {
            println!("{C_YELLOW}[WARN]{C_RESET} Unit not found at {}", unit_path.display());
        }
        run_cmd_expect_ok("systemctl", &["daemon-reload"]);
        // Quiet best-effort: suppress any "not loaded" noise
        let _ = run_cmd_quiet("systemctl", &["reset-failed", &service_name]);
    }

    // Remove binary
    section("Removing binary");
    if dry_run {
        println!("{C_CYAN}[DRY-RUN]{C_RESET} Would remove: {}", bin_path.display());
    } else if bin_path.exists() {
        println!("{C_GREEN}[INFO]{C_RESET} Removing binary {}", bin_path.display());
        if let Err(e) = fs::remove_file(&bin_path) {
            eprintln!("{C_YELLOW}[WARN]{C_RESET} remove {}: {}", bin_path.display(), e);
        }
    } else {
        println!("{C_YELLOW}[WARN]{C_RESET} Binary not found at {}", bin_path.display());
    }

    section("Done");
    println!("{C_BGREEN}[SUCCESS]{C_RESET} {} uninstalled.", service_name);
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────
fn section(title: &str) {
    println!("\n{C_BLUE}╔══════════════════════════════════════════════════════════╗{C_RESET}");
    println!("{C_BLUE}║{C_RESET} {C_WHITE}{C_BOLD}{:<54}{C_RESET} {C_BLUE}║{C_RESET}", title);
    println!("{C_BLUE}╚══════════════════════════════════════════════════════════╝{C_RESET}");
}

fn validate_corky_package_or_exit() {
    let cargo_toml = Path::new("Cargo.toml");
    if !cargo_toml.exists() {
        exit_error("No Cargo.toml found in the current directory.");
    }

    let content = match fs::read_to_string(cargo_toml) {
        Ok(c) => c,
        Err(e) => exit_error(&format!("Failed to read Cargo.toml: {}", e)),
    };

    let parsed: CargoToml = match toml::from_str(&content) {
        Ok(p) => p,
        Err(e) => exit_error(&format!("Failed to parse Cargo.toml: {}", e)),
    };

    let is_corky = parsed
        .corky
        .and_then(|c| c.is_corky_package)
        .unwrap_or(false);

    if !is_corky {
        eprintln!("{C_RED}[ERROR]{C_RESET} This does not appear to be a Corky package.");
        eprintln!("{C_WHITE}A Corky package must have [corky] with is_corky_package = true in Cargo.toml.{C_RESET}");
        std::process::exit(1);
    }
}

fn pkg_name_and_description() -> Option<(String, String)> {
    let cargo_toml = Path::new("Cargo.toml");
    let content = fs::read_to_string(cargo_toml).ok()?;
    let parsed: CargoToml = toml::from_str(&content).ok()?;

    let name = parsed
        .package
        .as_ref()
        .and_then(|p| p.name.clone())
        .unwrap_or_else(|| "corky".to_string());

    let description = parsed
        .package
        .as_ref()
        .and_then(|p| p.description.clone())
        .unwrap_or_else(|| "corky service".to_string());

    Some((name, description))
}

fn installing_user() -> String {
    if is_root() {
        if let Ok(sudo_user) = env::var("SUDO_USER") {
            if !sudo_user.is_empty() {
                return sudo_user;
            }
        }
        "root".to_string()
    } else {
        env::var("USER").unwrap_or_else(|_| "user".to_string())
    }
}

/// Look up the primary group for a given user.
/// Returns the group name, falling back to the username if lookup fails.
fn primary_group_for_user(username: &str) -> String {
    // Try using `id -gn <username>` to get the primary group name
    if let Ok(output) = Command::new("id")
        .args(["-gn", username])
        .output()
    {
        if output.status.success() {
            let group = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !group.is_empty() {
                return group;
            }
        }
    }
    // Fallback to username (common case where username == groupname)
    username.to_string()
}

fn run_cmd(cmd: &str, args: &[&str]) -> bool {
    let status = Command::new(cmd)
        .args(args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status();
    match status {
        Ok(s) => s.success(),
        Err(e) => {
            eprintln!("{C_RED}[ERROR]{C_RESET} Failed to execute {} {:?}: {}", cmd, args, e);
            false
        }
    }
}

/// Quiet best-effort command: swallows stdout/stderr and returns success flag.
fn run_cmd_quiet(cmd: &str, args: &[&str]) -> bool {
    let status = Command::new(cmd)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    status.map(|s| s.success()).unwrap_or(false)
}

fn run_cmd_expect_ok(cmd: &str, args: &[&str]) {
    if !run_cmd(cmd, args) {
        eprintln!("{C_RED}[ERROR]{C_RESET} Command failed: {} {:?}", cmd, args);
        std::process::exit(1);
    }
}

/// Compute a simple checksum (XOR-based hash) of a file for TOCTOU protection.
/// Not cryptographically secure, but sufficient to detect tampering between build and install.
fn compute_file_checksum(path: &Path) -> Option<String> {
    let data = fs::read(path).ok()?;
    // Simple checksum: length + XOR of all bytes in chunks
    let len = data.len();
    let mut xor_sum: u64 = 0;
    for chunk in data.chunks(8) {
        let mut val: u64 = 0;
        for (i, &byte) in chunk.iter().enumerate() {
            val |= (byte as u64) << (i * 8);
        }
        xor_sum ^= val;
    }
    Some(format!("{:016x}{:016x}", len, xor_sum))
}

// ─────────────────────────────────────────────────────────────────────────────
// systemctl / journalctl helpers
// ─────────────────────────────────────────────────────────────────────────────
fn list_corky_services() -> Vec<ServiceInfo> {
    let mut services = Vec::new();

    if let Ok(output) = Command::new("systemctl")
        .args(["--user", "list-unit-files", "corky-*.service", "--no-legend"])
        .output()
    {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                if let Some(info) = parse_corky_service(line, "user") {
                    services.push(info);
                }
            }
        }
    }

    if let Ok(output) = Command::new("systemctl")
        .args(["list-unit-files", "corky-*.service", "--no-legend"])
        .output()
    {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                if let Some(info) = parse_corky_service(line, "system") {
                    services.push(info);
                }
            }
        }
    }

    services
}

/// Parse corky-* services and label with "user" or "system"
fn parse_corky_service(line: &str, scope: &str) -> Option<ServiceInfo> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if !parts.is_empty() {
        let first_part = parts[0];
        if first_part.starts_with("corky-") && first_part.ends_with(".service") {
            let name = first_part.trim_end_matches(".service").to_string();
            return Some(ServiceInfo { name, scope: scope.to_string() });
        }
    }
    None
}

/// Service with its scope (system or user)
#[derive(Clone)]
struct ServiceInfo {
    name: String,
    scope: String,
}

/// Interactive prompt to select a service from a list.
fn interactive_select_service(services: &[ServiceInfo]) -> ServiceInfo {
    let options: Vec<String> = services
        .iter()
        .map(|s| format!("{} ({})", s.name.replace("corky-", ""), s.scope))
        .collect();

    match inquire::Select::new("Select a service:", options.clone()).prompt() {
        Ok(selected) => {
            let index = options.iter().position(|o| o == &selected)
                .expect("selected option must exist");
            services[index].clone()
        }
        Err(_) => {
            eprintln!("{C_YELLOW}[WARN]{C_RESET} Service selection cancelled.");
            std::process::exit(1);
        }
    }
}

/// Elevate privileges if dealing with a system service.
fn elevate_if_system_service(service_info: &ServiceInfo) {
    if service_info.scope == "system" && !is_root() {
        ensure_sudo_timestamp();
        let args: Vec<String> = env::args().skip(1).collect();
        elevate_privileges(&args, &[]);
    }
}

fn resolve_service(arg: Option<ServiceName>) -> ServiceInfo {
    let services = list_corky_services();

    if services.is_empty() {
        exit_error("No Corky services found. You may need to install a service first.");
    }

    match arg {
        Some(ServiceName::Auto) => {
            if services.len() == 1 {
                services[0].clone()
            } else {
                eprintln!("{C_YELLOW}[WARN]{C_RESET} Multiple services found. Please specify one:");
                for (i, s) in services.iter().enumerate() {
                    eprintln!("  {}. {} ({})", i + 1, s.name, s.scope);
                }
                std::process::exit(1);
            }
        }
        Some(ServiceName::All) => {
            exit_error("Cannot perform this operation on all services. Specify one.");
        }
        Some(ServiceName::Interactive) => {
            interactive_select_service(&services)
        }
        Some(ServiceName::Custom(name)) => {
            let name_with_prefix = ensure_corky_prefix(&name);

            let matches: Vec<_> = services
                .iter()
                .filter(|s| s.name == name_with_prefix)
                .collect();

            if matches.is_empty() {
                eprintln!("{C_RED}[ERROR]{C_RESET} No service found with name: {}", name_with_prefix);
                eprintln!("Available services:");
                for s in &services {
                    eprintln!("  {} ({})", s.name, s.scope);
                }
                std::process::exit(1);
            } else if matches.len() > 1 {
                eprintln!("{C_YELLOW}[WARN]{C_RESET} Multiple services match: {}", name_with_prefix);
                for (i, s) in matches.iter().enumerate() {
                    eprintln!("  {}. {} ({})", i + 1, s.name, s.scope);
                }
                std::process::exit(1);
            } else {
                matches[0].clone()
            }
        }
        None => {
            if services.len() == 1 {
                services[0].clone()
            } else {
                interactive_select_service(&services)
            }
        }
    }
}

fn run_systemctl(action: &str, service_info: &ServiceInfo) {
    elevate_if_system_service(service_info);

    let mut cmd = Command::new("systemctl");
    if service_info.scope == "user" {
        cmd.arg("--user");
    }

    println!(
        "{C_GREEN}[INFO]{C_RESET} Running: systemctl {} {}.service",
        action, service_info.name
    );

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
        match cmd.arg(action).arg(format!("{}.service", service_info.name)).output() {
            Ok(output) => {
                let exit_code = output.status.code().unwrap_or(1);
                if !output.stdout.is_empty() {
                    println!("{}", String::from_utf8_lossy(&output.stdout));
                }
                if !output.stderr.is_empty() {
                    eprintln!("{}", String::from_utf8_lossy(&output.stderr));
                }
                if exit_code == 0 {
                    let action_past = match action {
                        "start" => "started",
                        "stop" => "stopped",
                        "restart" => "restarted",
                        "enable" => "enabled",
                        "disable" => "disabled",
                        _ => "processed",
                    };
                    println!("{C_BGREEN}[OK]{C_RESET} Service {} {}", service_info.name, action_past);
                } else {
                    eprintln!(
                        "{C_RED}[ERROR]{C_RESET} Failed to {} service {}. Exit code: {}",
                        action, service_info.name, exit_code
                    );
                }
                std::process::exit(exit_code);
            }
            Err(e) => {
                eprintln!("{C_RED}[ERROR]{C_RESET} Failed to execute systemctl: {}", e);
                std::process::exit(1);
            }
        }
    }
}

fn run_systemctl_logs(service_info: &ServiceInfo) {
    elevate_if_system_service(service_info);

    let mut cmd = Command::new("journalctl");
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
