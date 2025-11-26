use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::{generate, Shell};
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
    CompletionItems {
        /// Command to complete (logs, status, etc.)
        cmd: String,
    },
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
            _ => Ok(ServiceName::Custom(s.to_string())),
        }
    }
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
fn elevate_privileges(args: &[String]) -> ! {
    if env::var_os(ENV_ELEVATED_FLAG).is_some() {
        eprintln!("{C_RED}Elevation loop detected; aborting.{C_RESET}");
        std::process::exit(1);
    }
    ensure_sudo_timestamp();
    let exe = env::current_exe().expect("Failed to get current executable path");
    let cwd = env::current_dir().ok();

    let mut cmd = Command::new(SUDO_BIN);
    cmd.env(ENV_ELEVATED_FLAG, "1")
        .arg(exe)
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
            for (service, scope) in list_corky_services() {
                println!("  {} ({})", service, scope);
            }
        }
        Commands::Completion { shell } => {
            generate_completion(*shell);
        }
        Commands::CompletionItems { cmd: _ } => {
                for (service, _) in list_corky_services() {
                let service_name = service.replace("corky-", "");
                println!("{}", service_name);
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

        // Single, informative line instead of an empty heading
        println!("{C_GREEN}[INFO]{C_RESET} Elevating with sudo to install system service…");
        let mut args: Vec<String> = env::args().skip(1).collect();
        if dry_run && !args.iter().any(|x| x == "--dry-run") {
            args.push("--dry-run".to_string());
        }
        elevate_privileges(&args);
    }

    // Phase 2 (root): install only — no duplicate validation/build sections
    let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
    let (pkg_name, description) = pkg_name_and_description().unwrap_or_else(|| {
        eprintln!("{C_YELLOW}[WARN]{C_RESET} Using fallback metadata (could not read Cargo.toml).");
        ("corky".to_string(), "corky service".to_string())
    });

    let target_bin = Path::new("target").join("release").join(&pkg_name);
    let install_bin = Path::new(BIN_PATH_SYSTEM).join(&pkg_name);
    let unit_path = Path::new(UNIT_DIR_SYSTEM).join(format!("{}.service", &pkg_name));

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
    let working_dir = if cwd.is_dir() { cwd } else { PathBuf::from("/") };
    let unit_contents = format!(
        r#"[Unit]
Description={description}
Wants=network-online.target
After=network-online.target

[Service]
User={user}
Group={user}
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
        user = installing_user(),
        workdir = working_dir.display(),
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
        println!("{C_CYAN}[DRY-RUN]{C_RESET} Would run: systemctl enable {}", pkg_name);
        println!("{C_CYAN}[DRY-RUN]{C_RESET} Would run: systemctl start {}", pkg_name);
        return;
    }
    run_cmd_expect_ok("systemctl", &["daemon-reload"]);
    run_cmd_expect_ok("systemctl", &["enable", &pkg_name]);
    let ok = run_cmd("systemctl", &["start", &pkg_name]);
    if !ok {
        eprintln!("{C_RED}[ERROR]{C_RESET} Failed to start service. Try: systemctl status {}", pkg_name);
        std::process::exit(1);
    }

    section("Done");
    println!("{C_BGREEN}[SUCCESS]{C_RESET} {} installed and started.", pkg_name);
    println!("• Binary: {}", install_bin.display());
    println!("• Unit:   {}", unit_path.display());
    println!("→ Manage with: systemctl [status|restart|stop] {}", pkg_name);
}

fn uninstall_system_service(dry_run: bool) {
    // Phase 1 (user): elevate, no empty headings.
    if !is_root() {
        println!("{C_GREEN}[INFO]{C_RESET} Elevating with sudo to uninstall system service…");
        let mut args: Vec<String> = env::args().skip(1).collect();
        if dry_run && !args.iter().any(|x| x == "--dry-run") {
            args.push("--dry-run".to_string());
        }
        elevate_privileges(&args);
    }

    // Phase 2 (root)
    section("Uninstall (system service)");
    let (pkg_name, _) =
        pkg_name_and_description().unwrap_or_else(|| ("corky".to_string(), "corky service".to_string()));
    let unit = format!("{}.service", &pkg_name);
    let unit_path = Path::new(UNIT_DIR_SYSTEM).join(&unit);
    let bin_path = Path::new(BIN_PATH_SYSTEM).join(&pkg_name);

    // Stop & disable if present
    section("Stopping & disabling");
    if dry_run {
        println!("{C_CYAN}[DRY-RUN]{C_RESET} Would run: systemctl stop {}", pkg_name);
        println!("{C_CYAN}[DRY-RUN]{C_RESET} Would run: systemctl disable {}", pkg_name);
    } else {
        // best-effort; we keep stdout/stderr inherited so the user sees real actions (like symlink removal)
        let _ = run_cmd("systemctl", &["stop", &pkg_name]);
        let _ = run_cmd("systemctl", &["disable", &pkg_name]);
    }

    // Remove unit + reload; use quiet reset-failed so no confusing error text
    section("Removing unit & reloading daemon");
    if dry_run {
        println!("{C_CYAN}[DRY-RUN]{C_RESET} Would remove: {}", unit_path.display());
        println!("{C_CYAN}[DRY-RUN]{C_RESET} Would run: systemctl daemon-reload");
        println!("{C_CYAN}[DRY-RUN]{C_RESET} Would run: systemctl reset-failed {}", pkg_name);
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
        // Quiet best-effort: suppress any “not loaded” noise
        let _ = run_cmd_quiet("systemctl", &["reset-failed", &pkg_name]);
    }

    // Remove binary
    section("Removing binary");
    if dry_run {
        println!("{C_CYAN}[DRY-RUN]{C_RESET} Would remove: {}", bin_path.display());
    } else {
        if bin_path.exists() {
            println!("{C_GREEN}[INFO]{C_RESET} Removing binary {}", bin_path.display());
            if let Err(e) = fs::remove_file(&bin_path) {
                eprintln!("{C_YELLOW}[WARN]{C_RESET} remove {}: {}", bin_path.display(), e);
            }
        } else {
            println!("{C_YELLOW}[WARN]{C_RESET} Binary not found at {}", bin_path.display());
        }
    }

    section("Done");
    println!("{C_BGREEN}[SUCCESS]{C_RESET} {} uninstalled.", pkg_name);
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
        eprintln!("{C_RED}[ERROR]{C_RESET} No Cargo.toml found in the current directory.");
        std::process::exit(1);
    }
    let content = fs::read_to_string(cargo_toml).unwrap_or_default();
    let ok = content.contains("[corky]") && content.contains("is_corky_package = true");
    if !ok {
        eprintln!("{C_RED}[ERROR]{C_RESET} This does not appear to be a Corky package.");
        eprintln!("{C_WHITE}A Corky package must have [corky] with is_corky_package = true in Cargo.toml.{C_RESET}");
        std::process::exit(1);
    }
}

fn pkg_name_and_description() -> Option<(String, String)> {
    let cargo_toml = Path::new("Cargo.toml");
    let content = fs::read_to_string(cargo_toml).ok()?;
    let mut in_package = false;
    let mut name: Option<String> = None;
    let mut desc: Option<String> = None;
    for line in content.lines() {
        let l = line.trim();
        if l.starts_with("[package]") {
            in_package = true;
            continue;
        }
        if in_package && l.starts_with('[') && l.ends_with(']') && l != "[package]" {
            break;
        }
        if in_package {
            if name.is_none() && l.starts_with("name") {
                if let Some(v) = extract_toml_str_value(l) {
                    name = Some(v);
                }
            }
            if desc.is_none() && l.starts_with("description") {
                if let Some(v) = extract_toml_str_value(l) {
                    desc = Some(v);
                }
            }
        }
    }
    Some((
        name.unwrap_or_else(|| "corky".to_string()),
        desc.unwrap_or_else(|| "corky service".to_string()),
    ))
}

fn extract_toml_str_value(line: &str) -> Option<String> {
    let parts: Vec<&str> = line.splitn(2, '=').collect();
    if parts.len() != 2 {
        return None;
    }
    let rhs = parts[1].trim();
    if rhs.starts_with('"') && rhs.ends_with('"') && rhs.len() >= 2 {
        return Some(rhs[1..rhs.len() - 1].to_string());
    }
    None
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

// ─────────────────────────────────────────────────────────────────────────────
// systemctl / journalctl helpers
// ─────────────────────────────────────────────────────────────────────────────
fn list_corky_services() -> Vec<(String, String)> {
    let mut services = Vec::new();

    if let Ok(output) = Command::new("systemctl")
        .args(["--user", "list-unit-files", "corky-*.service", "--no-legend"])
        .output()
    {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                if let Some(service_info) = parse_corky_service(line, "user") {
                    services.push(service_info);
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

/// Service with its scope (system or user)
struct ServiceInfo {
    name: String,
    scope: String,
}

fn resolve_service(arg: Option<ServiceName>) -> ServiceInfo {
    let services = list_corky_services();

    if services.is_empty() {
        eprintln!("{C_RED}[ERROR]{C_RESET} No Corky services found. You may need to install a service first.");
        std::process::exit(1);
    }

    let service_name = match arg {
        Some(ServiceName::Auto) => {
            if services.len() == 1 {
                services[0].0.clone()
            } else {
                eprintln!("{C_YELLOW}[WARN]{C_RESET} Multiple services found. Please specify one:");
                for (i, (service, scope)) in services.iter().enumerate() {
                    eprintln!("  {}. {} ({})", i + 1, service, scope);
                }
                std::process::exit(1);
            }
        }
        Some(ServiceName::All) => {
            eprintln!("{C_RED}[ERROR]{C_RESET} Cannot perform this operation on all services. Specify one.");
            std::process::exit(1);
        }
        Some(ServiceName::Interactive) => {
            if services.is_empty() {
                eprintln!("{C_RED}[ERROR]{C_RESET} No services to select from.");
                std::process::exit(1);
            }
            let options: Vec<String> = services
                .iter()
                .map(|(name, scope)| {
                    let display_name = name.replace("corky-", "");
                    format!("{} ({})", display_name, scope)
                })
                .collect();

            match inquire::Select::new("Select a service:", options.clone()).prompt() {
                Ok(selected) => {
                    let index = options.iter().position(|o| o == &selected).unwrap();
                    services[index].0.clone()
                }
                Err(_) => {
                    eprintln!("{C_YELLOW}[WARN]{C_RESET} Service selection cancelled.");
                    std::process::exit(1);
                }
            }
        }
        Some(ServiceName::Custom(name)) => {
            let name_with_prefix = if !name.starts_with("corky-") {
                format!("corky-{}", name)
            } else {
                name
            };

            let matches: Vec<_> = services
                .iter()
                .filter(|(service, _)| service == &name_with_prefix)
                .collect();

            if matches.is_empty() {
                eprintln!("{C_RED}[ERROR]{C_RESET} No service found with name: {}", name_with_prefix);
                eprintln!("Available services:");
                for (service, scope) in &services {
                    eprintln!("  {} ({})", service, scope);
                }
                std::process::exit(1);
            } else if matches.len() > 1 {
                eprintln!("{C_YELLOW}[WARN]{C_RESET} Multiple services match: {}", name_with_prefix);
                for (i, (service, scope)) in matches.iter().enumerate() {
                    eprintln!("  {}. {} ({})", i + 1, service, scope);
                }
                std::process::exit(1);
            } else {
                matches[0].0.clone()
            }
        }
        None => {
            if services.len() == 1 {
                services[0].0.clone()
            } else {
                let options: Vec<String> = services
                    .iter()
                    .map(|(name, scope)| {
                        let display_name = name.replace("corky-", "");
                        format!("{} ({})", display_name, scope)
                    })
                    .collect();

                match inquire::Select::new("Select a service:", options.clone()).prompt() {
                    Ok(selected) => {
                        let index = options.iter().position(|o| o == &selected).unwrap();
                        services[index].0.clone()
                    }
                    Err(_) => {
                        eprintln!("{C_YELLOW}[WARN]{C_RESET} Service selection cancelled.");
                        std::process::exit(1);
                    }
                }
            }
        }
    };

    let scope = services
        .iter()
        .find(|(name, _)| name == &service_name)
        .map(|(_, scope)| scope.clone())
        .unwrap_or_else(|| "system".to_string());

    ServiceInfo { name: service_name, scope }
}

fn run_systemctl(action: &str, service_info: &ServiceInfo) {
    if service_info.scope == "system" && !is_root() {
        ensure_sudo_timestamp();
        let args: Vec<String> = env::args().skip(1).collect();
        elevate_privileges(&args);
    }

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
    if service_info.scope == "system" && !is_root() {
        ensure_sudo_timestamp();
        let args: Vec<String> = env::args().skip(1).collect();
        elevate_privileges(&args);
    }

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
