#!/usr/bin/env bash
#
# uninstall.sh
#
# This script disables and stops any running services/executables for the
# package, removes all installed service files, preserves the existing
# ~/.corky/config.toml, and runs 'cargo clean'.
# It shows a list of removed files. It does NOT remove ~/.corky/config.toml.
#
# Usage: ./uninstall.sh [--dry-run]
#
###############################################################################

# IMMEDIATELY check for Cargo.toml before doing anything else
if [[ ! -f "Cargo.toml" ]]; then
  echo "Error: No Cargo.toml found in the current directory."
  echo "Please run this command from a Rust project directory containing a Cargo.toml file."
  exit 1
fi

# Check if this is a Corky package by looking for [corky] section and is_corky_package = true
if ! grep -q "\[corky\]" Cargo.toml || ! grep -q "is_corky_package *= *true" Cargo.toml; then
  echo -e "\033[1;31mError: This does not appear to be a Corky package.\033[0m"
  echo "Only Corky packages can be uninstalled with this script."
  echo "A Corky package must have [corky] section with is_corky_package = true in Cargo.toml."
  exit 1
fi

# Control banner display with environment variable to prevent double display
# when script re-executes with sudo
if [ -z "$BANNER_DISPLAYED" ]; then
  export BANNER_DISPLAYED=1
  SHOW_BANNER=true
else
  SHOW_BANNER=false
fi

# Base colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
BLUE='\033[0;34m'
MAGENTA='\033[0;35m'
CYAN='\033[0;36m'
WHITE='\033[0;37m'

# Bright/bold variants
BRED='\033[1;31m'
BGREEN='\033[1;32m'
BYELLOW='\033[1;33m'
BBLUE='\033[1;34m'
BMAGENTA='\033[1;35m'
BCYAN='\033[1;36m'
BWHITE='\033[1;37m'

# Text styles
BOLD='\033[1m'
UNDERLINE='\033[4m'
NC='\033[0m' # No Color

# Function to display usage information
function usage() {
  echo -e "${BWHITE}${BOLD}Usage:${NC} $0 [--dry-run]"
  echo
  echo -e "  ${BCYAN}--dry-run${NC}   Show what would happen without executing."
  echo
  exit 1
}

# Enhanced logging functions with improved colors
function log_info() {
  echo -e "${BGREEN}[INFO]${NC} ${WHITE}$1${NC}"
}

function log_warn() {
  echo -e "${BYELLOW}[WARN]${NC} ${WHITE}$1${NC}"
}

function log_error() {
  echo -e "${BRED}[ERROR]${NC} ${WHITE}$1${NC}"
}

function log_success() {
  echo -e "${BGREEN}[SUCCESS]${NC} ${WHITE}$1${NC}"
}

function log_section() {
  # Create a more visually distinct section header
  echo -e "\n${BLUE}╔══════════════════════════════════════════════════════════╗${NC}"
  echo -e "${BLUE}║${NC} ${BWHITE}${BOLD}$1${NC} ${BLUE}$(printf '%*s' $((47 - ${#1})) '')║${NC}"
  echo -e "${BLUE}╚══════════════════════════════════════════════════════════╝${NC}\n"
}

# Function for displaying dry-run information
function dry_run_echo() {
  echo -e "${BCYAN}[DRY-RUN]${NC} ${WHITE}$1${NC}"
}

# Check if this is a Rust project directory
function check_cargo_toml() {
  if [[ ! -f "Cargo.toml" ]]; then
    log_error "No Cargo.toml found in the current directory. Nothing to uninstall."
    log_info "Please run this command from a Rust project directory containing a Cargo.toml file."
    exit 1
  fi
}

# Function to parse and display the ASCII banner from Cargo.toml
function display_banner() {
  if [[ -f "Cargo.toml" ]]; then
    local in_banner=false
    local in_ascii=false
    local banner_content=""
    
    while IFS= read -r line; do
      if [[ "$line" =~ \[banner\] ]]; then
        in_banner=true
        continue
      fi
      
      if $in_banner && [[ "$line" =~ ^ascii[[:space:]]*=[[:space:]]*\"\"\" ]]; then
        in_ascii=true
        continue
      fi
      
      if $in_ascii; then
        if [[ "$line" =~ \"\"\" ]]; then
          break
        fi
        banner_content+="$line\n"
      fi
    done < Cargo.toml
    
    # Print the banner with additional spacing
    echo -e "\n$banner_content"
  else
    echo -e "${BRED}${BOLD}╔═════════════════════════════════════════╗${NC}"
    echo -e "${BRED}${BOLD}║         CORKY CHARTS UNINSTALLER       ║${NC}"
    echo -e "${BRED}${BOLD}╚═════════════════════════════════════════╝${NC}"
  fi
}

function get_installing_user() {
  if [[ -n "$SUDO_USER" ]]; then
    echo "$SUDO_USER"
  else
    echo "$USER"
  fi
}

# Parse the service-name from Cargo.toml
function get_service_name() {
  local in_package=false
  local result=""
  while IFS= read -r line; do
    if [[ "$line" =~ ^\s*\[package\]\s*$ ]]; then
      in_package=true
      continue
    fi
    if $in_package && [[ "$line" =~ ^name.*=.*\"(.+)\" ]]; then
      result="${BASH_REMATCH[1]}"
      break
    fi
  done < Cargo.toml

  if [[ -z "$result" ]]; then
    log_error "Could not find [package] name in Cargo.toml"
    exit 1
  fi

  echo "$result"
}


###############################################################################
# MAIN LOGIC
###############################################################################

# Initialize variables
DRY_RUN=false
VALID_ARGS=("--dry-run" "-h" "--help")

# Parse arguments first before displaying the banner
for arg in "$@"; do
  case "$arg" in
    --dry-run)
      DRY_RUN=true
      ;;
    -h|--help)
      usage
      ;;
    *)
      # Check if the argument is in the list of valid arguments
      VALID=false
      for valid_arg in "${VALID_ARGS[@]}"; do
        if [[ "$arg" == "$valid_arg" ]]; then
          VALID=true
          break
        fi
      done
      
      if ! $VALID; then
        log_error "Invalid argument: $arg"
        echo
        usage
      fi
      ;;
  esac
done

# Display the banner only if not suppressed
if $SHOW_BANNER; then
  display_banner
fi

# Check if we're in a Rust project directory
check_cargo_toml

# Special handling for two-phase uninstallation
# First phase: Run cargo clean as normal user, then elevate
# Second phase: Remove system files as root
if [[ -z "$PHASE2" && $EUID -ne 0 ]]; then
  # We're in phase 1 (normal user)
  log_section "Running cargo clean"
  
  if $DRY_RUN; then
    dry_run_echo "Would run: cargo clean"
    log_info "Ran cargo clean"
  else
    if cargo clean; then
      log_success "cargo clean completed successfully"
    else
      log_warn "cargo clean failed, but continuing with uninstallation"
    fi
  fi
  
  # Now elevate to root with sudo for phase 2
  log_info "Elevating privileges for system uninstallation..."
  
  if ! sudo -v; then
    log_error "Failed to obtain sudo privileges. Please run this script with sudo."
    exit 1
  fi
  
  echo -e "${BGREEN}Sudo access granted. Proceeding with uninstallation...${NC}"
  
  # Pass environment variables to phase 2
  sudo BANNER_DISPLAYED=1 PHASE2=true DRY_RUN=$DRY_RUN bash "$0"
  exit $?
fi

# If we get here, we're in phase 2 (running as root)

# Get service name from Cargo.toml
SERVICE_NAME="$(get_service_name)"
SYSTEMD_SERVICE_PATH="/etc/systemd/system/${SERVICE_NAME}.service"
INSTALLING_USER="$(get_installing_user)"
INSTALLING_USER_HOME="$(getent passwd "$INSTALLING_USER" | cut -d: -f6)"
CORKY_PATH="$INSTALLING_USER_HOME/.corky"
BIN_PATH="$CORKY_PATH/bin"

# Stop and disable service
log_section "Stopping and disabling service"

if $DRY_RUN; then
  dry_run_echo "Would run: systemctl stop ${SERVICE_NAME}.service"
  dry_run_echo "Would run: systemctl disable ${SERVICE_NAME}.service"
else
  log_info "Stopping service..."
  systemctl stop "${SERVICE_NAME}.service" 2>/dev/null || true
  
  log_info "Disabling service..."
  systemctl disable "${SERVICE_NAME}.service" 2>/dev/null || true
fi

# Remove systemd service file
log_section "Removing service files"

if $DRY_RUN; then
  dry_run_echo "Would remove: $SYSTEMD_SERVICE_PATH"
else
  log_info "Removing systemd service file..."
  if [[ -f "$SYSTEMD_SERVICE_PATH" ]]; then
    rm -f "$SYSTEMD_SERVICE_PATH"
    log_success "Removed $SYSTEMD_SERVICE_PATH"
  else
    log_warn "Service file not found at $SYSTEMD_SERVICE_PATH"
  fi
  
  log_info "Reloading systemd daemon..."
  systemctl daemon-reload
fi

# Remove installed binaries
log_section "Removing installed binaries"

EXECUTABLE_PATH="$BIN_PATH/$SERVICE_NAME"

if $DRY_RUN; then
  dry_run_echo "Would remove: $EXECUTABLE_PATH"
else
  log_info "Removing executable..."
  if [[ -f "$EXECUTABLE_PATH" ]]; then
    rm -f "$EXECUTABLE_PATH"
    log_success "Removed $EXECUTABLE_PATH"
  else
    log_warn "Executable not found at $EXECUTABLE_PATH"
  fi
fi

# We already ran 'cargo clean' in phase 1, so we don't need to do it again here

log_section "Uninstallation Complete"
log_success "$SERVICE_NAME has been uninstalled!"
log_info "The following were preserved:"
log_info "- Configuration at: $CORKY_PATH/config.toml (if it existed)"
echo
log_info "To reinstall, use: corky install"
