#!/usr/bin/env bash
set -euo pipefail

# ── Tephra Installer ─────────────────────────────────────────────────────────
# Interactive installer for tephra-server and/or tephra-client.
# Handles Rust toolchain, system dependencies, building, and installation.

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m'

info()  { echo -e "${BLUE}::${NC} $*"; }
ok()    { echo -e "${GREEN}::${NC} $*"; }
warn()  { echo -e "${YELLOW}::${NC} $*"; }
error() { echo -e "${RED}::${NC} $*"; }

banner() {
    echo -e "${CYAN}"
    echo "  ╔════════════════════════════════════════╗"
    echo "  ║            TEPHRA INSTALLER            ║"
    echo "  ║   CPU Thermal Monitoring for Linux     ║"
    echo "  ╚════════════════════════════════════════╝"
    echo -e "${NC}"
}

# ── Detect environment ───────────────────────────────────────────────────────

detect_distro() {
    if [ -f /etc/os-release ]; then
        . /etc/os-release
        echo "$ID"
    else
        echo "unknown"
    fi
}

DISTRO=$(detect_distro)
ARCH=$(uname -m)

# ── Dependency helpers ───────────────────────────────────────────────────────

install_rust() {
    if command -v cargo &>/dev/null; then
        ok "Rust toolchain found: $(rustc --version)"
        return 0
    fi

    info "Rust toolchain not found. Installing via rustup..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source "$HOME/.cargo/env"
    ok "Rust installed: $(rustc --version)"
}

install_client_deps() {
    info "Checking system dependencies for the GUI client..."

    case "$DISTRO" in
        arch|cachyos|endeavouros|manjaro|garuda)
            local pkgs=()
            for pkg in vulkan-icd-loader pkgconf cmake fontconfig freetype2; do
                if ! pacman -Qi "$pkg" &>/dev/null; then
                    pkgs+=("$pkg")
                fi
            done
            if [ ${#pkgs[@]} -gt 0 ]; then
                info "Installing: ${pkgs[*]}"
                if command -v paru &>/dev/null; then
                    paru -S --needed --noconfirm "${pkgs[@]}"
                else
                    sudo pacman -S --needed --noconfirm "${pkgs[@]}"
                fi
            fi
            ;;
        ubuntu|debian|pop|linuxmint)
            local pkgs=()
            for pkg in libvulkan-dev pkg-config cmake libfontconfig1-dev libfreetype6-dev; do
                if ! dpkg -s "$pkg" &>/dev/null 2>&1; then
                    pkgs+=("$pkg")
                fi
            done
            if [ ${#pkgs[@]} -gt 0 ]; then
                info "Installing: ${pkgs[*]}"
                sudo apt-get update -qq
                sudo apt-get install -y "${pkgs[@]}"
            fi
            ;;
        fedora|rhel|centos|rocky|alma)
            local pkgs=()
            for pkg in vulkan-loader-devel pkgconf cmake fontconfig-devel freetype-devel; do
                if ! rpm -q "$pkg" &>/dev/null 2>&1; then
                    pkgs+=("$pkg")
                fi
            done
            if [ ${#pkgs[@]} -gt 0 ]; then
                info "Installing: ${pkgs[*]}"
                sudo dnf install -y "${pkgs[@]}"
            fi
            ;;
        opensuse*)
            local pkgs=()
            for pkg in libvulkan1 pkg-config cmake fontconfig-devel freetype2-devel; do
                if ! rpm -q "$pkg" &>/dev/null 2>&1; then
                    pkgs+=("$pkg")
                fi
            done
            if [ ${#pkgs[@]} -gt 0 ]; then
                info "Installing: ${pkgs[*]}"
                sudo zypper install -y "${pkgs[@]}"
            fi
            ;;
        *)
            warn "Unknown distro '$DISTRO'. Please ensure Vulkan drivers, pkg-config,"
            warn "cmake, fontconfig, and freetype are installed."
            read -rp "Continue anyway? [y/N] " yn
            [[ "$yn" =~ ^[Yy] ]] || exit 1
            ;;
    esac

    ok "Client dependencies satisfied"
}

# ── Build & Install ──────────────────────────────────────────────────────────

build_server() {
    info "Building tephra server (release)..."
    cargo build --release -p tephra-server
    ok "Server built successfully"
}

build_client() {
    info "Building tephra client (release)..."
    cargo build --release -p tephra-client
    ok "Client built successfully"
}

install_server() {
    info "Installing tephra server..."

    sudo cp target/release/tephra /usr/local/bin/tephra
    sudo chmod +x /usr/local/bin/tephra
    ok "Binary installed to /usr/local/bin/tephra"

    # Install systemd service
    if command -v systemctl &>/dev/null; then
        echo ""
        read -rp "$(echo -e "${BLUE}::${NC} Install and start systemd service? [Y/n] ")" yn
        if [[ ! "$yn" =~ ^[Nn] ]]; then
            sudo cp tephra.service /etc/systemd/system/tephra.service
            sudo systemctl daemon-reload
            sudo systemctl enable tephra
            sudo systemctl start tephra
            ok "Service installed and started"
            info "  Status:  systemctl status tephra"
            info "  Logs:    journalctl -u tephra -f"
            info "  Test:    curl http://localhost:9867/health"
        fi
    else
        warn "systemd not found — skipping service installation"
        info "Run manually: tephra --port 9867"
    fi
}

install_client() {
    info "Installing tephra client..."

    sudo cp target/release/tephra-client /usr/local/bin/tephra-client
    sudo chmod +x /usr/local/bin/tephra-client
    ok "Binary installed to /usr/local/bin/tephra-client"
    info "Run with: tephra-client"
}

# ── Main ─────────────────────────────────────────────────────────────────────

banner

echo -e "  Detected: ${BOLD}$DISTRO${NC} on ${BOLD}$ARCH${NC}"
echo ""
echo -e "  What would you like to install?"
echo ""
echo -e "    ${BOLD}1${NC})  Server only   — monitoring agent (runs on each machine)"
echo -e "    ${BOLD}2${NC})  Client only   — GUI dashboard (runs on your desktop)"
echo -e "    ${BOLD}3${NC})  Both          — server + client"
echo ""

read -rp "  Choose [1/2/3]: " choice

case "$choice" in
    1) INSTALL_SERVER=true;  INSTALL_CLIENT=false ;;
    2) INSTALL_SERVER=false; INSTALL_CLIENT=true  ;;
    3) INSTALL_SERVER=true;  INSTALL_CLIENT=true  ;;
    *)
        error "Invalid choice. Exiting."
        exit 1
        ;;
esac

echo ""

# Rust toolchain
install_rust

# Client system deps (only if installing client)
if $INSTALL_CLIENT; then
    install_client_deps
fi

echo ""

# Build
if $INSTALL_SERVER; then
    build_server
fi
if $INSTALL_CLIENT; then
    build_client
fi

echo ""

# Install
if $INSTALL_SERVER; then
    install_server
fi
if $INSTALL_CLIENT; then
    install_client
fi

echo ""
echo -e "${GREEN}  ╔════════════════════════════════════════╗${NC}"
echo -e "${GREEN}  ║        Installation complete!          ║${NC}"
echo -e "${GREEN}  ╚════════════════════════════════════════╝${NC}"
echo ""

if $INSTALL_SERVER; then
    info "Server: tephra is running on port 9867"
fi
if $INSTALL_CLIENT; then
    info "Client: run ${BOLD}tephra-client${NC} to launch the dashboard"
fi
echo ""
