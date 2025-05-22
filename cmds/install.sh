#!/usr/bin/env bash
#
# install.sh
# 
# This script installs the service as a systemd unit, stores files in the
# installing user's home directory (not root's home), and ensures the
# service is enabled and started on system boot.
#
# Usage: ./install.sh [--dry-run]
#
# If --dry-run is provided, it will show the actions that would be taken
# without actually performing them.
#

# IMMEDIATELY check for Cargo.toml before doing anything else
if [[ ! -f "Cargo.toml" ]]; then
  echo "Error: No Cargo.toml found in the current directory."
  echo "Please run this command from a Rust project directory containing a Cargo.toml file."
  exit 1
fi

# Check if this is a Corky package by looking for [corky] section and is_corky_package = true
if ! grep -q "\[corky\]" Cargo.toml || ! grep -q "is_corky_package *= *true" Cargo.toml; then
  echo -e "\033[1;31mError: This does not appear to be a Corky package.\033[0m"
  echo "Only Corky packages can be installed with this script."
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

###############################################################################
# COLOR DEFINITIONS
###############################################################################
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

###############################################################################
# HELPER FUNCTIONS
###############################################################################

function usage() {
  echo -e "${BOLD}Usage:${NC} $0 [--dry-run]"
  echo
  echo "  --dry-run   Show what would happen without executing."
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
    log_error "No Cargo.toml found in the current directory. Nothing to install."
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
      
      # If we hit a new section, stop processing banner
      if $in_banner && [[ "$line" =~ ^\[.*\] ]]; then
        break
      fi
    done < Cargo.toml
    
    # Print the banner with additional spacing
    echo -e "\n$banner_content"
  else
    echo -e "${BRED}${BOLD}╔═════════════════════════════════════════╗${NC}"
    echo -e "${BRED}${BOLD}║            CORKY INSTALLER             ║${NC}"
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
# MAIN SCRIPT
###############################################################################

# Display the banner only if not suppressed
if $SHOW_BANNER; then
  display_banner
fi

# Parse arguments
DRY_RUN=false
VALID_ARGS=("--dry-run" "-h" "--help")

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

# Check if we're in a Rust project directory
check_cargo_toml

# Special handling for two-phase installation
# First phase: Build as normal user, then elevate
# Second phase: Install as root
if [[ -z "$PHASE2" && $EUID -ne 0 ]]; then
  # We're in phase 1 (normal user)
  log_section "Building Release Version of Package"
  
  if $DRY_RUN; then
    dry_run_echo "Would run: cargo build --release"
  else
    log_info "Running cargo build --release..."
    if ! cargo build --release; then
      log_error "cargo build --release failed."
      exit 1
    fi
    log_success "Build completed successfully."
  fi
  
  # Now elevate to root with sudo for phase 2
  log_info "Elevating privileges for system installation..."
  
  if ! sudo -v; then
    log_error "Failed to obtain sudo privileges. Please run this script with sudo."
    exit 1
  fi
  
  echo -e "${BGREEN}Sudo access granted. Proceeding with installation...${NC}"
  
  # Pass environment variables to phase 2
  sudo BANNER_DISPLAYED=1 PHASE2=true DRY_RUN=$DRY_RUN bash "$0"
  exit $?
fi

# If we get here, we're in phase 2 (running as root)

INSTALLING_USER="$(get_installing_user)"
INSTALLING_USER_HOME="$(getent passwd "$INSTALLING_USER" | cut -d: -f6)"
CORKY_PATH="$INSTALLING_USER_HOME/.corky"
BIN_PATH="$CORKY_PATH/bin"
CONFIG_PATH="$CORKY_PATH/config.toml"
PKG_CONFIG="./config.toml"  # Assuming 'config.toml' is in the same directory as Cargo.toml
SERVICE_NAME="$(get_service_name)"
SYSTEMD_SERVICE_PATH="/etc/systemd/system/${SERVICE_NAME}.service"

# Removed uninstall section to prevent error messages when uninstall.sh is not found

# Skip the build step as we already did it in phase 1
log_section "Installing Binary"

# Create installation directories
if $DRY_RUN; then
  dry_run_echo "Would create directory: $CORKY_PATH"
  dry_run_echo "Would create directory: $BIN_PATH"
else
  log_info "Creating installation directories..."
  mkdir -p "$BIN_PATH"
  chown -R "$INSTALLING_USER:$INSTALLING_USER" "$CORKY_PATH"
fi

# Copy executable
if $DRY_RUN; then
  dry_run_echo "Would copy: ./target/release/$SERVICE_NAME to $BIN_PATH/"
else
  log_info "Copying executable..."
  cp "./target/release/$SERVICE_NAME" "$BIN_PATH/"
  chmod +x "$BIN_PATH/$SERVICE_NAME"
  chown "$INSTALLING_USER:$INSTALLING_USER" "$BIN_PATH/$SERVICE_NAME"
  log_success "Executable installed to $BIN_PATH/$SERVICE_NAME"
fi

# Create systemd service unit
log_section "Installing Systemd Service"

# Extract description from Cargo.toml
DESCRIPTION=$(grep -E '^\s*description\s*=\s*".*"' Cargo.toml | head -1 | sed 's/.*=\s*"\(.*\)".*/\1/')
if [[ -z "$DESCRIPTION" ]]; then
  DESCRIPTION="$SERVICE_NAME service"
fi

# Generate systemd unit file content
SYSTEMD_CONTENT="[Unit]
Description=$DESCRIPTION
After=network.target

[Service]
ExecStart=$BIN_PATH/$SERVICE_NAME
Restart=on-failure
User=$INSTALLING_USER
Group=$INSTALLING_USER
WorkingDirectory=$INSTALLING_USER_HOME
Environment=PATH=$BIN_PATH:/usr/local/bin:/usr/bin:/bin

[Install]
WantedBy=multi-user.target
"

if $DRY_RUN; then
  dry_run_echo "Would create systemd service file: $SYSTEMD_SERVICE_PATH"
  echo -e "${BCYAN}[DRY-RUN] Service file content:${NC}"
  echo "$SYSTEMD_CONTENT"
else
  log_info "Creating systemd service file..."
  echo "$SYSTEMD_CONTENT" > "$SYSTEMD_SERVICE_PATH"
  chmod 644 "$SYSTEMD_SERVICE_PATH"
  log_success "Service unit created at $SYSTEMD_SERVICE_PATH"
fi

# Enable and start the service
log_section "Enabling and Starting Service"

if $DRY_RUN; then
  dry_run_echo "Would run: systemctl daemon-reload"
  dry_run_echo "Would run: systemctl enable $SERVICE_NAME"
  dry_run_echo "Would run: systemctl start $SERVICE_NAME"
else
  log_info "Reloading systemd daemon..."
  systemctl daemon-reload
  
  log_info "Enabling service to start on boot..."
  systemctl enable "$SERVICE_NAME"
  
  log_info "Starting service..."
  if systemctl start "$SERVICE_NAME"; then
    log_success "Service $SERVICE_NAME started successfully!"
  else
    log_error "Failed to start service. Check status with: systemctl status $SERVICE_NAME"
  fi
fi

log_section "Installation Complete"
log_success "$SERVICE_NAME has been installed!"
log_info "You can control it with the following commands:"
echo -e " - ${BCYAN}systemctl status $SERVICE_NAME${NC}"
echo -e " - ${BCYAN}systemctl start $SERVICE_NAME${NC}"
echo -e " - ${BCYAN}systemctl stop $SERVICE_NAME${NC}"
echo -e " - ${BCYAN}systemctl restart $SERVICE_NAME${NC}"
echo
log_info "Configuration is located at: $CONFIG_PATH"
log_info "Binary is installed at: $BIN_PATH/$SERVICE_NAME"
echo
log_info "To uninstall, use: corky uninstall"
