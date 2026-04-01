use serde::Deserialize;
use std::env;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

// ─────────────────────────────────────────────────────────────────────────────
// Constants & colors
// ─────────────────────────────────────────────────────────────────────────────
pub const SUDO_BIN: &str = "sudo";
pub const ENV_ELEVATED_FLAG: &str = "CORKY_ELEVATED";
pub const ENV_BINARY_CHECKSUM: &str = "CORKY_BINARY_CHECKSUM";
pub const ENV_ORIGINAL_CWD: &str = "CORKY_ORIGINAL_CWD";
pub const ENV_INIT_BACKEND: &str = "CORKY_INIT_BACKEND";
pub const BIN_PATH_SYSTEM: &str = "/usr/local/bin";
const UNIT_DIR_SYSTEM: &str = "/etc/systemd/system";
const SUPERVISOR_CONF_DIR: &str = "/etc/supervisor/conf.d";

pub const C_RESET: &str = "\x1b[0m";
pub const C_BOLD: &str = "\x1b[1m";
pub const C_GREEN: &str = "\x1b[32m";
pub const C_BGREEN: &str = "\x1b[1;32m";
pub const C_RED: &str = "\x1b[31m";
pub const C_YELLOW: &str = "\x1b[33m";
pub const C_BLUE: &str = "\x1b[34m";
pub const C_WHITE: &str = "\x1b[37m";
pub const C_CYAN: &str = "\x1b[36m";

// ─────────────────────────────────────────────────────────────────────────────
// Init backend enum
// ─────────────────────────────────────────────────────────────────────────────

/// Which init system manages services on this host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InitBackend {
    Systemd { scope: String },
    Supervisor,
}

impl std::fmt::Display for InitBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InitBackend::Systemd { .. } => write!(f, "systemd"),
            InitBackend::Supervisor => write!(f, "supervisor"),
        }
    }
}

impl InitBackend {
    pub fn display_label(&self) -> String {
        match self {
            InitBackend::Systemd { scope } => format!("systemd/{}", scope),
            InitBackend::Supervisor => "supervisor".to_string(),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// ServiceInfo
// ─────────────────────────────────────────────────────────────────────────────

/// A discovered corky service with its managing backend.
#[derive(Clone, Debug)]
pub struct ServiceInfo {
    pub name: String,
    pub backend: InitBackend,
}

// ─────────────────────────────────────────────────────────────────────────────
// Detection
// ─────────────────────────────────────────────────────────────────────────────

/// Detect which init system is available.
/// Priority: CORKY_INIT_BACKEND env > systemd > supervisor > error.
pub fn detect_backend() -> InitBackend {
    // 1. Explicit override via environment variable
    if let Ok(val) = env::var(ENV_INIT_BACKEND) {
        match val.to_lowercase().as_str() {
            "systemd" => {
                return InitBackend::Systemd {
                    scope: detect_systemd_scope(),
                };
            }
            "supervisor" | "supervisord" => return InitBackend::Supervisor,
            other => {
                eprintln!(
                    "{C_YELLOW}[WARN]{C_RESET} Unknown {}='{}', auto-detecting.",
                    ENV_INIT_BACKEND, other
                );
            }
        }
    }

    // 2. systemd: check /run/systemd/system
    if is_systemd_available() {
        return InitBackend::Systemd {
            scope: detect_systemd_scope(),
        };
    }

    // 3. supervisor: check socket then binary
    if is_supervisor_available() {
        return InitBackend::Supervisor;
    }

    // 4. Neither found
    exit_error(
        &format!(
            "No supported init system detected (checked systemd, supervisor).\n\
             Set {}=systemd|supervisor to override.",
            ENV_INIT_BACKEND
        ),
    );
}

fn detect_systemd_scope() -> String {
    if is_root() { "system".to_string() } else { "user".to_string() }
}

fn is_systemd_available() -> bool {
    Path::new("/run/systemd/system").exists()
}

fn is_supervisor_available() -> bool {
    // Check for supervisord socket
    if Path::new("/var/run/supervisor.sock").exists() {
        return true;
    }
    if Path::new("/run/supervisor.sock").exists() {
        return true;
    }
    // Fallback: try supervisorctl
    run_cmd_quiet("supervisorctl", &["version"])
}

/// Warn about orphaned configs from a different init system.
pub fn check_migration_warning(backend: &InitBackend) {
    match backend {
        InitBackend::Supervisor => {
            // Check for orphaned systemd units
            if let Ok(entries) = fs::read_dir(UNIT_DIR_SYSTEM) {
                let orphans: Vec<_> = entries
                    .flatten()
                    .filter(|e| {
                        let n = e.file_name();
                        let s = n.to_string_lossy();
                        s.starts_with("corky-") && s.ends_with(".service")
                    })
                    .collect();
                if !orphans.is_empty() {
                    eprintln!(
                        "{C_YELLOW}[WARN]{C_RESET} Found {} orphaned systemd unit file(s) but running under supervisor.",
                        orphans.len()
                    );
                    for o in &orphans {
                        eprintln!("  rm {}", o.path().display());
                    }
                }
            }
        }
        InitBackend::Systemd { .. } => {
            // Check for orphaned supervisor configs
            if let Ok(entries) = fs::read_dir(SUPERVISOR_CONF_DIR) {
                let orphans: Vec<_> = entries
                    .flatten()
                    .filter(|e| {
                        let n = e.file_name();
                        let s = n.to_string_lossy();
                        s.starts_with("corky-") && s.ends_with(".conf")
                    })
                    .collect();
                if !orphans.is_empty() {
                    eprintln!(
                        "{C_YELLOW}[WARN]{C_RESET} Found {} orphaned supervisor config(s) but running under systemd.",
                        orphans.len()
                    );
                    for o in &orphans {
                        eprintln!("  rm {}", o.path().display());
                    }
                }
            }
        }
    }
}

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
// Service listing
// ─────────────────────────────────────────────────────────────────────────────

pub fn list_corky_services(backend: &InitBackend) -> Vec<ServiceInfo> {
    match backend {
        InitBackend::Systemd { .. } => list_corky_services_systemd(),
        InitBackend::Supervisor => list_corky_services_supervisor(),
    }
}

fn list_corky_services_systemd() -> Vec<ServiceInfo> {
    let mut services = Vec::new();

    // User-scope services
    if let Ok(output) = Command::new("systemctl")
        .args(["--user", "list-unit-files", "corky-*.service", "--no-legend"])
        .output()
    {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                if let Some(info) = parse_systemd_service(line, "user") {
                    services.push(info);
                }
            }
        }
    }

    // System-scope services
    if let Ok(output) = Command::new("systemctl")
        .args(["list-unit-files", "corky-*.service", "--no-legend"])
        .output()
    {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                if let Some(info) = parse_systemd_service(line, "system") {
                    services.push(info);
                }
            }
        }
    }

    services
}

fn parse_systemd_service(line: &str, scope: &str) -> Option<ServiceInfo> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if !parts.is_empty() {
        let first_part = parts[0];
        if first_part.starts_with("corky-") && first_part.ends_with(".service") {
            let name = first_part.trim_end_matches(".service").to_string();
            return Some(ServiceInfo {
                name,
                backend: InitBackend::Systemd {
                    scope: scope.to_string(),
                },
            });
        }
    }
    None
}

fn list_corky_services_supervisor() -> Vec<ServiceInfo> {
    let mut services = Vec::new();

    // Method 1: Parse supervisorctl status output
    if let Ok(output) = Command::new("supervisorctl").arg("status").output() {
        // exit code 3 = some processes not running, still valid output
        if output.status.success() || output.status.code() == Some(3) {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                let name = line.split_whitespace().next().unwrap_or("");
                if name.starts_with("corky-") {
                    services.push(ServiceInfo {
                        name: name.to_string(),
                        backend: InitBackend::Supervisor,
                    });
                }
            }
        }
    }

    // Method 2: Scan config directory for configs not yet loaded
    if let Ok(entries) = fs::read_dir(SUPERVISOR_CONF_DIR) {
        for entry in entries.flatten() {
            let file_name = entry.file_name();
            let fname = file_name.to_string_lossy();
            if fname.starts_with("corky-") && fname.ends_with(".conf") {
                let svc_name = fname.trim_end_matches(".conf").to_string();
                if !services.iter().any(|s| s.name == svc_name) {
                    services.push(ServiceInfo {
                        name: svc_name,
                        backend: InitBackend::Supervisor,
                    });
                }
            }
        }
    }

    services
}

// ─────────────────────────────────────────────────────────────────────────────
// Service actions (start / stop / restart / status / enable / disable)
// ─────────────────────────────────────────────────────────────────────────────

pub fn run_service_action(action: &str, service_info: &ServiceInfo) {
    elevate_if_needed(service_info);

    match &service_info.backend {
        InitBackend::Systemd { scope } => {
            let mut cmd = Command::new("systemctl");
            if scope == "user" {
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
                match cmd
                    .arg(action)
                    .arg(format!("{}.service", service_info.name))
                    .output()
                {
                    Ok(output) => {
                        let exit_code = output.status.code().unwrap_or(1);
                        if !output.stdout.is_empty() {
                            println!("{}", String::from_utf8_lossy(&output.stdout));
                        }
                        if !output.stderr.is_empty() {
                            eprintln!("{}", String::from_utf8_lossy(&output.stderr));
                        }
                        if exit_code == 0 {
                            println!(
                                "{C_BGREEN}[OK]{C_RESET} Service {} {}",
                                service_info.name,
                                past_tense(action)
                            );
                        } else {
                            eprintln!(
                                "\n{C_RED}[ERROR]{C_RESET} Failed to {} service {}. Exit code: {}",
                                action, service_info.name, exit_code
                            );
                        }
                        std::process::exit(exit_code);
                    }
                    Err(e) => {
                        eprintln!(
                            "{C_RED}[ERROR]{C_RESET} Failed to execute systemctl: {}",
                            e
                        );
                        std::process::exit(1);
                    }
                }
            }
        }
        InitBackend::Supervisor => {
            println!(
                "{C_GREEN}[INFO]{C_RESET} Running: supervisorctl {} {}",
                action, service_info.name
            );

            let status = Command::new("supervisorctl")
                .args([action, &service_info.name])
                .stdin(Stdio::inherit())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .status()
                .expect("Failed to run supervisorctl");

            let exit_code = status.code().unwrap_or(1);
            if action != "status" {
                if exit_code == 0 {
                    println!(
                        "{C_BGREEN}[OK]{C_RESET} Service {} {}",
                        service_info.name,
                        past_tense(action)
                    );
                } else {
                    eprintln!(
                        "\n{C_RED}[ERROR]{C_RESET} Failed to {} service {}. Exit code: {}",
                        action, service_info.name, exit_code
                    );
                }
            }
            std::process::exit(exit_code);
        }
    }
}

/// Enable a service (auto-start).
pub fn run_service_enable(service_info: &ServiceInfo) {
    elevate_if_needed(service_info);

    match &service_info.backend {
        InitBackend::Systemd { scope } => {
            let mut cmd = Command::new("systemctl");
            if scope == "user" {
                cmd.arg("--user");
            }
            println!(
                "{C_GREEN}[INFO]{C_RESET} Running: systemctl enable {}.service",
                service_info.name
            );
            match cmd
                .arg("enable")
                .arg(format!("{}.service", service_info.name))
                .output()
            {
                Ok(output) => {
                    if !output.stdout.is_empty() {
                        println!("{}", String::from_utf8_lossy(&output.stdout));
                    }
                    if !output.stderr.is_empty() {
                        eprintln!("{}", String::from_utf8_lossy(&output.stderr));
                    }
                    if output.status.success() {
                        println!(
                            "{C_BGREEN}[OK]{C_RESET} Service {} enabled",
                            service_info.name
                        );
                    } else {
                        eprintln!(
                            "{C_RED}[ERROR]{C_RESET} Failed to enable {}",
                            service_info.name
                        );
                        std::process::exit(1);
                    }
                }
                Err(e) => {
                    eprintln!("{C_RED}[ERROR]{C_RESET} Failed to run systemctl: {}", e);
                    std::process::exit(1);
                }
            }
        }
        InitBackend::Supervisor => {
            let conf_path = supervisor_conf_path(&service_info.name);
            if !conf_path.exists() {
                exit_error(&format!(
                    "Supervisor config not found: {}. Is the service installed?",
                    conf_path.display()
                ));
            }
            set_supervisor_autostart(&conf_path, true);
            run_cmd_expect_ok("supervisorctl", &["reread"]);
            run_cmd_expect_ok("supervisorctl", &["update"]);
            println!(
                "{C_BGREEN}[OK]{C_RESET} Service {} enabled (autostart=true)",
                service_info.name
            );
        }
    }
}

/// Disable a service (no auto-start).
pub fn run_service_disable(service_info: &ServiceInfo) {
    elevate_if_needed(service_info);

    match &service_info.backend {
        InitBackend::Systemd { scope } => {
            let mut cmd = Command::new("systemctl");
            if scope == "user" {
                cmd.arg("--user");
            }
            println!(
                "{C_GREEN}[INFO]{C_RESET} Running: systemctl disable {}.service",
                service_info.name
            );
            match cmd
                .arg("disable")
                .arg(format!("{}.service", service_info.name))
                .output()
            {
                Ok(output) => {
                    if !output.stdout.is_empty() {
                        println!("{}", String::from_utf8_lossy(&output.stdout));
                    }
                    if !output.stderr.is_empty() {
                        eprintln!("{}", String::from_utf8_lossy(&output.stderr));
                    }
                    if output.status.success() {
                        println!(
                            "{C_BGREEN}[OK]{C_RESET} Service {} disabled",
                            service_info.name
                        );
                    } else {
                        eprintln!(
                            "{C_RED}[ERROR]{C_RESET} Failed to disable {}",
                            service_info.name
                        );
                        std::process::exit(1);
                    }
                }
                Err(e) => {
                    eprintln!("{C_RED}[ERROR]{C_RESET} Failed to run systemctl: {}", e);
                    std::process::exit(1);
                }
            }
        }
        InitBackend::Supervisor => {
            let conf_path = supervisor_conf_path(&service_info.name);
            if !conf_path.exists() {
                exit_error(&format!(
                    "Supervisor config not found: {}. Is the service installed?",
                    conf_path.display()
                ));
            }
            set_supervisor_autostart(&conf_path, false);
            run_cmd_expect_ok("supervisorctl", &["reread"]);
            run_cmd_expect_ok("supervisorctl", &["update"]);
            println!(
                "{C_BGREEN}[OK]{C_RESET} Service {} disabled (autostart=false)",
                service_info.name
            );
        }
    }
}

/// Follow/stream logs for a service.
pub fn run_service_logs(service_info: &ServiceInfo) -> ! {
    elevate_if_needed(service_info);

    match &service_info.backend {
        InitBackend::Systemd { scope } => {
            let mut cmd = Command::new("journalctl");
            if scope == "user" {
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
        InitBackend::Supervisor => {
            let status = Command::new("supervisorctl")
                .args(["tail", "-f", &service_info.name])
                .stdin(Stdio::inherit())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .status()
                .expect("Failed to run supervisorctl tail");
            std::process::exit(status.code().unwrap_or(1));
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Install / Uninstall
// ─────────────────────────────────────────────────────────────────────────────

pub fn install_service(backend: &InitBackend, dry_run: bool, skip_init: bool) {
    // Step 1: Validate
    section("Validating Corky package");
    validate_corky_package_or_exit();
    println!(
        "{C_BGREEN}[OK]{C_RESET} {C_WHITE}Found Cargo.toml with [corky] is_corky_package=true{C_RESET}"
    );

    // Step 2: Build
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

    // Step 3: Compute checksum
    let (raw_pkg_name, _) = pkg_name_and_description()
        .unwrap_or_else(|| ("corky".to_string(), "corky service".to_string()));
    let target_bin = Path::new("target").join("release").join(&raw_pkg_name);
    let checksum = if !dry_run {
        compute_file_checksum(&target_bin).unwrap_or_else(|| {
            eprintln!(
                "{C_RED}[ERROR]{C_RESET} Failed to compute checksum of {}",
                target_bin.display()
            );
            std::process::exit(1);
        })
    } else {
        String::new()
    };

    // Step 4: Elevate if needed, passing backend and checksum through env
    if !is_root() {
        println!("{C_GREEN}[INFO]{C_RESET} Elevating with sudo to install service...");
        let args: Vec<String> = env::args().skip(1).collect();
        let backend_str = backend.to_string();
        elevate_privileges(
            &args,
            &[
                (ENV_BINARY_CHECKSUM, &checksum),
                (ENV_INIT_BACKEND, &backend_str),
            ],
        );
    }

    // Phase 2 (root): install binary and config
    let cwd = env::var(ENV_ORIGINAL_CWD)
        .ok()
        .map(PathBuf::from)
        .unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| PathBuf::from("/")));

    if let Err(e) = env::set_current_dir(&cwd) {
        eprintln!(
            "{C_YELLOW}[WARN]{C_RESET} Could not change to original directory {}: {}",
            cwd.display(),
            e
        );
    }

    let (raw_pkg_name, description) = pkg_name_and_description().unwrap_or_else(|| {
        eprintln!("{C_YELLOW}[WARN]{C_RESET} Using fallback metadata (could not read Cargo.toml).");
        ("corky".to_string(), "corky service".to_string())
    });

    let service_name = ensure_corky_prefix(&raw_pkg_name);
    let target_bin = Path::new("target").join("release").join(&raw_pkg_name);
    let install_bin = Path::new(BIN_PATH_SYSTEM).join(&raw_pkg_name);

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

    // Install binary
    section("Installing binary");
    if dry_run {
        println!(
            "{C_CYAN}[DRY-RUN]{C_RESET} Would install: {} -> {} (0755)",
            target_bin.display(),
            install_bin.display()
        );
    } else {
        let data = fs::read(&target_bin).unwrap_or_else(|e| {
            eprintln!(
                "{C_RED}[ERROR]{C_RESET} Read {}: {}",
                target_bin.display(),
                e
            );
            std::process::exit(1);
        });
        fs::create_dir_all(BIN_PATH_SYSTEM).unwrap_or_else(|e| {
            eprintln!("{C_RED}[ERROR]{C_RESET} create {}: {}", BIN_PATH_SYSTEM, e);
            std::process::exit(1);
        });
        if let Err(e) = fs::write(&install_bin, &data) {
            // ETXTBSY: binary is currently running -- stop it first, then retry
            if e.raw_os_error() == Some(26) {
                eprintln!(
                    "{C_YELLOW}[WARN]{C_RESET} Binary is running. Stopping {} before overwriting...",
                    service_name
                );
                match backend {
                    InitBackend::Systemd { .. } => {
                        let _ = run_cmd("systemctl", &["stop", &service_name]);
                    }
                    InitBackend::Supervisor => {
                        let _ = run_cmd("supervisorctl", &["stop", &service_name]);
                    }
                }
                fs::write(&install_bin, &data).unwrap_or_else(|e2| {
                    eprintln!(
                        "{C_RED}[ERROR]{C_RESET} write {} (after stop): {}",
                        install_bin.display(),
                        e2
                    );
                    std::process::exit(1);
                });
            } else {
                eprintln!(
                    "{C_RED}[ERROR]{C_RESET} write {}: {}",
                    install_bin.display(),
                    e
                );
                std::process::exit(1);
            }
        }
        let mut perms = fs::metadata(&install_bin)
            .map(|m| m.permissions())
            .unwrap_or_else(|e| {
                eprintln!(
                    "{C_RED}[ERROR]{C_RESET} stat {}: {}",
                    install_bin.display(),
                    e
                );
                std::process::exit(1);
            });
        perms.set_mode(0o755);
        fs::set_permissions(&install_bin, perms).unwrap_or_else(|e| {
            eprintln!(
                "{C_RED}[ERROR]{C_RESET} chmod {}: {}",
                install_bin.display(),
                e
            );
            std::process::exit(1);
        });
        println!("{C_GREEN}[INFO]{C_RESET} Installed: {}", install_bin.display());
    }

    // Write init-system config
    if !cwd.is_dir() {
        eprintln!(
            "{C_RED}[ERROR]{C_RESET} Working directory does not exist: {}",
            cwd.display()
        );
        eprintln!("Please run this command from a valid directory.");
        std::process::exit(1);
    }

    let user = installing_user();
    let group = primary_group_for_user(&user);

    match backend {
        InitBackend::Systemd { .. } => {
            install_systemd_config(
                dry_run,
                skip_init,
                &service_name,
                &description,
                &install_bin,
                &cwd,
                &user,
                &group,
            );
        }
        InitBackend::Supervisor => {
            install_supervisor_config(
                dry_run,
                skip_init,
                &service_name,
                &description,
                &install_bin,
                &cwd,
                &user,
            );
        }
    }
}

fn install_systemd_config(
    dry_run: bool,
    skip_init: bool,
    service_name: &str,
    description: &str,
    install_bin: &Path,
    cwd: &Path,
    user: &str,
    group: &str,
) {
    let unit_path = Path::new(UNIT_DIR_SYSTEM).join(format!("{}.service", service_name));

    section("Writing systemd unit");
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
        println!(
            "{C_CYAN}[DRY-RUN]{C_RESET} Would write unit: {}",
            unit_path.display()
        );
        println!(
            "---------- unit file ----------\n{}\n-------------------------------",
            unit_contents
        );
    } else {
        fs::write(&unit_path, unit_contents).unwrap_or_else(|e| {
            eprintln!(
                "{C_RED}[ERROR]{C_RESET} write {}: {}",
                unit_path.display(),
                e
            );
            std::process::exit(1);
        });
        // Best-effort SELinux relabel
        let _ = Command::new("restorecon")
            .arg("-v")
            .arg(&unit_path)
            .status();
        let _ = Command::new("restorecon")
            .arg("-v")
            .arg(install_bin)
            .status();
        println!(
            "{C_GREEN}[INFO]{C_RESET} Wrote unit: {}",
            unit_path.display()
        );
    }

    section("Reloading & enabling service");
    if dry_run {
        if skip_init {
            println!("{C_CYAN}[DRY-RUN]{C_RESET} --skip-init: Would skip systemctl commands");
        } else {
            println!("{C_CYAN}[DRY-RUN]{C_RESET} Would run: systemctl daemon-reload");
            println!(
                "{C_CYAN}[DRY-RUN]{C_RESET} Would run: systemctl enable {}",
                service_name
            );
            println!(
                "{C_CYAN}[DRY-RUN]{C_RESET} Would run: systemctl start {}",
                service_name
            );
        }
    } else if skip_init {
        println!("{C_YELLOW}[SKIP]{C_RESET} systemctl daemon-reload");
        println!("{C_YELLOW}[SKIP]{C_RESET} systemctl enable {}", service_name);
        println!("{C_YELLOW}[SKIP]{C_RESET} systemctl start {}", service_name);
    } else {
        run_cmd_expect_ok("systemctl", &["daemon-reload"]);
        run_cmd_expect_ok("systemctl", &["enable", service_name]);
        let ok = run_cmd("systemctl", &["start", service_name]);
        if !ok {
            eprintln!(
                "{C_RED}[ERROR]{C_RESET} Failed to start service. Try: systemctl status {}",
                service_name
            );
            std::process::exit(1);
        }
    }

    section("Done");
    if skip_init && !dry_run {
        println!(
            "{C_BGREEN}[SUCCESS]{C_RESET} {} installed (binary + unit file).",
            service_name
        );
        println!("  Binary: {}", install_bin.display());
        println!("  Unit:   {}", unit_path.display());
        println!();
        println!("To run manually:");
        println!("  cd {} && {}", cwd.display(), install_bin.display());
    } else {
        println!(
            "{C_BGREEN}[SUCCESS]{C_RESET} {} installed and started.",
            service_name
        );
        println!("  Binary: {}", install_bin.display());
        println!("  Unit:   {}", unit_path.display());
        println!(
            "  Manage: systemctl [status|restart|stop] {}",
            service_name
        );
    }
}

fn install_supervisor_config(
    dry_run: bool,
    skip_init: bool,
    service_name: &str,
    description: &str,
    install_bin: &Path,
    cwd: &Path,
    user: &str,
) {
    let conf_path = supervisor_conf_path(service_name);

    section("Writing supervisor config");
    let conf_contents = generate_supervisor_conf(service_name, description, install_bin, cwd, user);

    if dry_run {
        println!(
            "{C_CYAN}[DRY-RUN]{C_RESET} Would write config: {}",
            conf_path.display()
        );
        println!(
            "---------- supervisor config ----------\n{}\n---------------------------------------",
            conf_contents
        );
    } else {
        fs::create_dir_all(SUPERVISOR_CONF_DIR).unwrap_or_else(|e| {
            eprintln!(
                "{C_RED}[ERROR]{C_RESET} create {}: {}",
                SUPERVISOR_CONF_DIR, e
            );
            std::process::exit(1);
        });
        fs::write(&conf_path, conf_contents).unwrap_or_else(|e| {
            eprintln!(
                "{C_RED}[ERROR]{C_RESET} write {}: {}",
                conf_path.display(),
                e
            );
            std::process::exit(1);
        });
        println!(
            "{C_GREEN}[INFO]{C_RESET} Wrote config: {}",
            conf_path.display()
        );
    }

    section("Registering & starting service");
    if dry_run {
        if skip_init {
            println!(
                "{C_CYAN}[DRY-RUN]{C_RESET} --skip-init: Would skip supervisorctl commands"
            );
        } else {
            println!("{C_CYAN}[DRY-RUN]{C_RESET} Would run: supervisorctl reread");
            println!("{C_CYAN}[DRY-RUN]{C_RESET} Would run: supervisorctl update");
            println!(
                "{C_CYAN}[DRY-RUN]{C_RESET} Would run: supervisorctl start {}",
                service_name
            );
        }
    } else if skip_init {
        println!("{C_YELLOW}[SKIP]{C_RESET} supervisorctl reread");
        println!("{C_YELLOW}[SKIP]{C_RESET} supervisorctl update");
        println!(
            "{C_YELLOW}[SKIP]{C_RESET} supervisorctl start {}",
            service_name
        );
    } else {
        run_cmd_expect_ok("supervisorctl", &["reread"]);
        run_cmd_expect_ok("supervisorctl", &["update"]);
        // After update, supervisor may auto-start the service (autostart=true).
        // Explicitly start to be sure.
        let ok = run_cmd("supervisorctl", &["start", service_name]);
        if !ok {
            // Not fatal -- update may have already started it
            eprintln!(
                "{C_YELLOW}[WARN]{C_RESET} supervisorctl start returned non-zero (may already be running)"
            );
        }
    }

    section("Done");
    if skip_init && !dry_run {
        println!(
            "{C_BGREEN}[SUCCESS]{C_RESET} {} installed (binary + config).",
            service_name
        );
        println!("  Binary: {}", install_bin.display());
        println!("  Config: {}", conf_path.display());
        println!();
        println!("To activate:");
        println!("  supervisorctl reread && supervisorctl update");
    } else {
        println!(
            "{C_BGREEN}[SUCCESS]{C_RESET} {} installed and started.",
            service_name
        );
        println!("  Binary: {}", install_bin.display());
        println!("  Config: {}", conf_path.display());
        println!(
            "  Manage: supervisorctl [status|restart|stop] {}",
            service_name
        );
    }
}

pub fn uninstall_service(backend: &InitBackend, dry_run: bool, skip_init: bool) {
    // Elevate if needed
    if !is_root() {
        println!("{C_GREEN}[INFO]{C_RESET} Elevating with sudo to uninstall service...");
        let args: Vec<String> = env::args().skip(1).collect();
        let backend_str = backend.to_string();
        elevate_privileges(&args, &[(ENV_INIT_BACKEND, &backend_str)]);
    }

    if let Ok(original_cwd) = env::var(ENV_ORIGINAL_CWD) {
        let _ = env::set_current_dir(&original_cwd);
    }

    section("Uninstall service");
    let (raw_pkg_name, _) = pkg_name_and_description()
        .unwrap_or_else(|| ("corky".to_string(), "corky service".to_string()));
    let service_name = ensure_corky_prefix(&raw_pkg_name);
    let bin_path = Path::new(BIN_PATH_SYSTEM).join(&raw_pkg_name);

    match backend {
        InitBackend::Systemd { .. } => {
            uninstall_systemd(dry_run, skip_init, &service_name, &bin_path);
        }
        InitBackend::Supervisor => {
            uninstall_supervisor(dry_run, skip_init, &service_name, &bin_path);
        }
    }
}

fn uninstall_systemd(dry_run: bool, skip_init: bool, service_name: &str, bin_path: &Path) {
    let unit_path = Path::new(UNIT_DIR_SYSTEM).join(format!("{}.service", service_name));

    // Stop & disable
    section("Stopping & disabling");
    if dry_run {
        if skip_init {
            println!(
                "{C_CYAN}[DRY-RUN]{C_RESET} --skip-init: Would skip systemctl commands"
            );
        } else {
            println!(
                "{C_CYAN}[DRY-RUN]{C_RESET} Would run: systemctl stop {}",
                service_name
            );
            println!(
                "{C_CYAN}[DRY-RUN]{C_RESET} Would run: systemctl disable {}",
                service_name
            );
        }
    } else if skip_init {
        println!("{C_YELLOW}[SKIP]{C_RESET} systemctl stop {}", service_name);
        println!(
            "{C_YELLOW}[SKIP]{C_RESET} systemctl disable {}",
            service_name
        );
    } else {
        let _ = run_cmd("systemctl", &["stop", service_name]);
        let _ = run_cmd("systemctl", &["disable", service_name]);
    }

    // Remove unit + reload
    section("Removing unit & reloading daemon");
    if dry_run {
        println!(
            "{C_CYAN}[DRY-RUN]{C_RESET} Would remove: {}",
            unit_path.display()
        );
        if !skip_init {
            println!("{C_CYAN}[DRY-RUN]{C_RESET} Would run: systemctl daemon-reload");
            println!(
                "{C_CYAN}[DRY-RUN]{C_RESET} Would run: systemctl reset-failed {}",
                service_name
            );
        }
    } else {
        if unit_path.exists() {
            println!(
                "{C_GREEN}[INFO]{C_RESET} Removing unit {}",
                unit_path.display()
            );
            if let Err(e) = fs::remove_file(&unit_path) {
                eprintln!(
                    "{C_YELLOW}[WARN]{C_RESET} remove {}: {}",
                    unit_path.display(),
                    e
                );
            }
        } else {
            println!(
                "{C_YELLOW}[WARN]{C_RESET} Unit not found at {}",
                unit_path.display()
            );
        }
        if skip_init {
            println!("{C_YELLOW}[SKIP]{C_RESET} systemctl daemon-reload");
            println!(
                "{C_YELLOW}[SKIP]{C_RESET} systemctl reset-failed {}",
                service_name
            );
        } else {
            run_cmd_expect_ok("systemctl", &["daemon-reload"]);
            let _ = run_cmd_quiet("systemctl", &["reset-failed", service_name]);
        }
    }

    // Remove binary
    remove_binary(dry_run, bin_path);

    section("Done");
    println!(
        "{C_BGREEN}[SUCCESS]{C_RESET} {} uninstalled.",
        service_name
    );
}

fn uninstall_supervisor(dry_run: bool, skip_init: bool, service_name: &str, bin_path: &Path) {
    let conf_path = supervisor_conf_path(service_name);

    // Stop
    section("Stopping service");
    if dry_run {
        if skip_init {
            println!(
                "{C_CYAN}[DRY-RUN]{C_RESET} --skip-init: Would skip supervisorctl commands"
            );
        } else {
            println!(
                "{C_CYAN}[DRY-RUN]{C_RESET} Would run: supervisorctl stop {}",
                service_name
            );
        }
    } else if skip_init {
        println!(
            "{C_YELLOW}[SKIP]{C_RESET} supervisorctl stop {}",
            service_name
        );
    } else {
        let _ = run_cmd("supervisorctl", &["stop", service_name]);
    }

    // Remove config + reread/update
    section("Removing config & updating supervisor");
    if dry_run {
        println!(
            "{C_CYAN}[DRY-RUN]{C_RESET} Would remove: {}",
            conf_path.display()
        );
        if !skip_init {
            println!("{C_CYAN}[DRY-RUN]{C_RESET} Would run: supervisorctl reread");
            println!("{C_CYAN}[DRY-RUN]{C_RESET} Would run: supervisorctl update");
        }
    } else {
        if conf_path.exists() {
            println!(
                "{C_GREEN}[INFO]{C_RESET} Removing config {}",
                conf_path.display()
            );
            if let Err(e) = fs::remove_file(&conf_path) {
                eprintln!(
                    "{C_YELLOW}[WARN]{C_RESET} remove {}: {}",
                    conf_path.display(),
                    e
                );
            }
        } else {
            println!(
                "{C_YELLOW}[WARN]{C_RESET} Config not found at {}",
                conf_path.display()
            );
        }
        if skip_init {
            println!("{C_YELLOW}[SKIP]{C_RESET} supervisorctl reread");
            println!("{C_YELLOW}[SKIP]{C_RESET} supervisorctl update");
        } else {
            let _ = run_cmd("supervisorctl", &["reread"]);
            let _ = run_cmd("supervisorctl", &["update"]);
        }
    }

    // Remove binary
    remove_binary(dry_run, bin_path);

    section("Done");
    println!(
        "{C_BGREEN}[SUCCESS]{C_RESET} {} uninstalled.",
        service_name
    );
}

fn remove_binary(dry_run: bool, bin_path: &Path) {
    section("Removing binary");
    if dry_run {
        println!(
            "{C_CYAN}[DRY-RUN]{C_RESET} Would remove: {}",
            bin_path.display()
        );
    } else if bin_path.exists() {
        println!(
            "{C_GREEN}[INFO]{C_RESET} Removing binary {}",
            bin_path.display()
        );
        if let Err(e) = fs::remove_file(bin_path) {
            eprintln!(
                "{C_YELLOW}[WARN]{C_RESET} remove {}: {}",
                bin_path.display(),
                e
            );
        }
    } else {
        println!(
            "{C_YELLOW}[WARN]{C_RESET} Binary not found at {}",
            bin_path.display()
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Supervisor helpers
// ─────────────────────────────────────────────────────────────────────────────

fn supervisor_conf_path(service_name: &str) -> PathBuf {
    PathBuf::from(SUPERVISOR_CONF_DIR).join(format!("{}.conf", service_name))
}

fn generate_supervisor_conf(
    service_name: &str,
    description: &str,
    exec_path: &Path,
    working_dir: &Path,
    user: &str,
) -> String {
    format!(
        r#"; {description}
; Managed by corky CLI -- do not edit manually
[program:{service_name}]
command={exec_path}
directory={working_dir}
user={user}
environment=RUST_LOG_STYLE="always"
autostart=true
autorestart=true
startsecs=1
startretries=3
redirect_stderr=true
stdout_logfile=/var/log/supervisor/{service_name}.log
stdout_logfile_maxbytes=10MB
stdout_logfile_backups=5
stopsignal=TERM
stopwaitsecs=10
stopasgroup=true
killasgroup=true
"#,
        description = description,
        service_name = service_name,
        exec_path = exec_path.display(),
        working_dir = working_dir.display(),
        user = user,
    )
}

/// Toggle autostart= in a supervisor .conf file.
fn set_supervisor_autostart(conf_path: &Path, enabled: bool) {
    let content = fs::read_to_string(conf_path).unwrap_or_else(|e| {
        exit_error(&format!("Read {}: {}", conf_path.display(), e));
    });

    let value = if enabled { "true" } else { "false" };
    let mut found = false;
    let new_content: String = content
        .lines()
        .map(|line| {
            let trimmed = line.trim();
            if trimmed.starts_with("autostart") && trimmed.contains('=') {
                found = true;
                format!("autostart={}", value)
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    let final_content = if found {
        format!("{}\n", new_content)
    } else {
        format!("{}\nautostart={}\n", new_content.trim_end(), value)
    };

    fs::write(conf_path, final_content).unwrap_or_else(|e| {
        exit_error(&format!("Write {}: {}", conf_path.display(), e));
    });
}

// ─────────────────────────────────────────────────────────────────────────────
// Service resolution & interactive selection
// ─────────────────────────────────────────────────────────────────────────────

/// Service name specification - can be a special value or a custom name.
#[derive(Debug, Clone)]
pub enum ServiceName {
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

fn is_valid_service_name(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

pub fn resolve_service(backend: &InitBackend, arg: Option<ServiceName>) -> ServiceInfo {
    let services = list_corky_services(backend);

    if services.is_empty() {
        exit_error("No Corky services found. You may need to install a service first.");
    }

    match arg {
        Some(ServiceName::Auto) => {
            if services.len() == 1 {
                services[0].clone()
            } else {
                eprintln!(
                    "{C_YELLOW}[WARN]{C_RESET} Multiple services found. Please specify one:"
                );
                for (i, s) in services.iter().enumerate() {
                    eprintln!("  {}. {} ({})", i + 1, s.name, s.backend.display_label());
                }
                std::process::exit(1);
            }
        }
        Some(ServiceName::All) => {
            exit_error("Cannot perform this operation on all services. Specify one.");
        }
        Some(ServiceName::Interactive) => interactive_select_service(&services),
        Some(ServiceName::Custom(name)) => {
            let name_with_prefix = ensure_corky_prefix(&name);
            let matches: Vec<_> = services
                .iter()
                .filter(|s| s.name == name_with_prefix)
                .collect();

            if matches.is_empty() {
                eprintln!(
                    "{C_RED}[ERROR]{C_RESET} No service found with name: {}",
                    name_with_prefix
                );
                eprintln!("Available services:");
                for s in &services {
                    eprintln!("  {} ({})", s.name, s.backend.display_label());
                }
                std::process::exit(1);
            } else if matches.len() > 1 {
                eprintln!(
                    "{C_YELLOW}[WARN]{C_RESET} Multiple services match: {}",
                    name_with_prefix
                );
                for (i, s) in matches.iter().enumerate() {
                    eprintln!("  {}. {} ({})", i + 1, s.name, s.backend.display_label());
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

fn interactive_select_service(services: &[ServiceInfo]) -> ServiceInfo {
    let options: Vec<String> = services
        .iter()
        .map(|s| {
            format!(
                "{} ({})",
                s.name.replace("corky-", ""),
                s.backend.display_label()
            )
        })
        .collect();

    match inquire::Select::new("Select a service:", options.clone()).prompt() {
        Ok(selected) => {
            let index = options
                .iter()
                .position(|o| o == &selected)
                .expect("selected option must exist");
            services[index].clone()
        }
        Err(_) => {
            eprintln!("{C_YELLOW}[WARN]{C_RESET} Service selection cancelled.");
            std::process::exit(1);
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Privilege helpers
// ─────────────────────────────────────────────────────────────────────────────

pub fn is_root() -> bool {
    #[cfg(unix)]
    {
        unsafe { libc::geteuid() == 0 }
    }
    #[cfg(not(unix))]
    {
        false
    }
}

pub fn ensure_sudo_timestamp() {
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

pub fn elevate_privileges(args: &[String], extra_env: &[(&str, &str)]) -> ! {
    if env::var_os(ENV_ELEVATED_FLAG).is_some() {
        eprintln!("{C_RED}Elevation loop detected; aborting.{C_RESET}");
        std::process::exit(1);
    }
    ensure_sudo_timestamp();
    let exe = env::current_exe().expect("Failed to get current executable path");

    let cwd = env::current_dir().ok();
    let cwd_str = cwd.as_ref().map(|p| p.to_string_lossy().to_string());

    // Pass env vars as KEY=VALUE arguments to sudo (before the command).
    // This is more reliable than cmd.env() because sudo's default env_reset
    // policy strips inherited environment variables.
    let mut cmd = Command::new(SUDO_BIN);

    cmd.arg(format!("{}=1", ENV_ELEVATED_FLAG));

    if let Some(ref dir) = cwd_str {
        cmd.arg(format!("{}={}", ENV_ORIGINAL_CWD, dir));
    }

    for (key, value) in extra_env {
        cmd.arg(format!("{}={}", key, value));
    }

    cmd.arg(exe.as_os_str())
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

fn elevate_if_needed(service_info: &ServiceInfo) {
    let backend_str = service_info.backend.to_string();
    match &service_info.backend {
        InitBackend::Systemd { scope } => {
            if scope == "system" && !is_root() {
                ensure_sudo_timestamp();
                let args: Vec<String> = env::args().skip(1).collect();
                elevate_privileges(&args, &[(ENV_INIT_BACKEND, &backend_str)]);
            }
        }
        InitBackend::Supervisor => {
            // Supervisor: check if we can access the socket
            if !is_root() && !run_cmd_quiet("supervisorctl", &["pid"]) {
                ensure_sudo_timestamp();
                let args: Vec<String> = env::args().skip(1).collect();
                elevate_privileges(&args, &[(ENV_INIT_BACKEND, &backend_str)]);
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Generic helpers
// ─────────────────────────────────────────────────────────────────────────────

pub fn exit_error(msg: &str) -> ! {
    eprintln!("{C_RED}[ERROR]{C_RESET} {}", msg);
    std::process::exit(1);
}

pub fn ensure_corky_prefix(name: &str) -> String {
    if name.starts_with("corky-") {
        name.to_string()
    } else {
        format!("corky-{}", name)
    }
}

pub fn section(title: &str) {
    println!(
        "\n{C_BLUE}\u{2554}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2557}{C_RESET}"
    );
    println!(
        "{C_BLUE}\u{2551}{C_RESET} {C_WHITE}{C_BOLD}{:<54}{C_RESET} {C_BLUE}\u{2551}{C_RESET}",
        title
    );
    println!(
        "{C_BLUE}\u{255a}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{255d}{C_RESET}"
    );
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
            eprintln!(
                "{C_RED}[ERROR]{C_RESET} Failed to execute {} {:?}: {}",
                cmd, args, e
            );
            false
        }
    }
}

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
        eprintln!(
            "{C_RED}[ERROR]{C_RESET} Command failed: {} {:?}",
            cmd, args
        );
        std::process::exit(1);
    }
}

fn past_tense(action: &str) -> &str {
    match action {
        "start" => "started",
        "stop" => "stopped",
        "restart" => "restarted",
        "enable" => "enabled",
        "disable" => "disabled",
        _ => "processed",
    }
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
        eprintln!(
            "{C_WHITE}A Corky package must have [corky] with is_corky_package = true in Cargo.toml.{C_RESET}"
        );
        std::process::exit(1);
    }
}

pub fn pkg_name_and_description() -> Option<(String, String)> {
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

fn primary_group_for_user(username: &str) -> String {
    if let Ok(output) = Command::new("id").args(["-gn", username]).output() {
        if output.status.success() {
            let group = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !group.is_empty() {
                return group;
            }
        }
    }
    username.to_string()
}

pub fn compute_file_checksum(path: &Path) -> Option<String> {
    let data = fs::read(path).ok()?;
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
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_backend_env_override_supervisor() {
        let prev = env::var(ENV_INIT_BACKEND).ok();
        unsafe { env::set_var(ENV_INIT_BACKEND, "supervisor") };
        let backend = detect_backend();
        assert_eq!(backend, InitBackend::Supervisor);
        match prev {
            Some(v) => unsafe { env::set_var(ENV_INIT_BACKEND, v) },
            None => unsafe { env::remove_var(ENV_INIT_BACKEND) },
        }
    }

    #[test]
    fn test_detect_backend_env_override_systemd() {
        let prev = env::var(ENV_INIT_BACKEND).ok();
        unsafe { env::set_var(ENV_INIT_BACKEND, "systemd") };
        let backend = detect_backend();
        assert!(matches!(backend, InitBackend::Systemd { .. }));
        match prev {
            Some(v) => unsafe { env::set_var(ENV_INIT_BACKEND, v) },
            None => unsafe { env::remove_var(ENV_INIT_BACKEND) },
        }
    }

    #[test]
    fn test_init_backend_display() {
        let systemd = InitBackend::Systemd {
            scope: "system".to_string(),
        };
        assert_eq!(format!("{}", systemd), "systemd");
        assert_eq!(systemd.display_label(), "systemd/system");

        let supervisor = InitBackend::Supervisor;
        assert_eq!(format!("{}", supervisor), "supervisor");
        assert_eq!(supervisor.display_label(), "supervisor");
    }

    #[test]
    fn test_ensure_corky_prefix() {
        assert_eq!(ensure_corky_prefix("zmq"), "corky-zmq");
        assert_eq!(ensure_corky_prefix("corky-zmq"), "corky-zmq");
        assert_eq!(ensure_corky_prefix("telegram"), "corky-telegram");
    }

    #[test]
    fn test_service_name_parsing() {
        assert!(matches!("auto".parse::<ServiceName>(), Ok(ServiceName::Auto)));
        assert!(matches!("all".parse::<ServiceName>(), Ok(ServiceName::All)));
        assert!(matches!(
            "interactive".parse::<ServiceName>(),
            Ok(ServiceName::Interactive)
        ));
        assert!(matches!(
            "zmq".parse::<ServiceName>(),
            Ok(ServiceName::Custom(_))
        ));
        assert!("foo bar".parse::<ServiceName>().is_err());
    }

    #[test]
    fn test_is_valid_service_name() {
        assert!(is_valid_service_name("corky-zmq"));
        assert!(is_valid_service_name("my_service"));
        assert!(is_valid_service_name("test123"));
        assert!(!is_valid_service_name(""));
        assert!(!is_valid_service_name("bad name"));
        assert!(!is_valid_service_name("bad;name"));
    }

    #[test]
    fn test_past_tense() {
        assert_eq!(past_tense("start"), "started");
        assert_eq!(past_tense("stop"), "stopped");
        assert_eq!(past_tense("restart"), "restarted");
        assert_eq!(past_tense("enable"), "enabled");
        assert_eq!(past_tense("disable"), "disabled");
        assert_eq!(past_tense("unknown"), "processed");
    }

    #[test]
    fn test_supervisor_conf_path() {
        let path = supervisor_conf_path("corky-zmq");
        assert_eq!(
            path,
            PathBuf::from("/etc/supervisor/conf.d/corky-zmq.conf")
        );
    }

    #[test]
    fn test_generate_supervisor_conf() {
        let conf = generate_supervisor_conf(
            "corky-test",
            "Test service",
            Path::new("/usr/local/bin/corky-test"),
            Path::new("/opt/corky"),
            "appuser",
        );
        assert!(conf.contains("[program:corky-test]"));
        assert!(conf.contains("command=/usr/local/bin/corky-test"));
        assert!(conf.contains("directory=/opt/corky"));
        assert!(conf.contains("user=appuser"));
        assert!(conf.contains("autostart=true"));
        assert!(conf.contains("autorestart=true"));
        assert!(conf.contains("stdout_logfile=/var/log/supervisor/corky-test.log"));
    }
}
