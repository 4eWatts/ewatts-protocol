#!/bin/bash
set -euo pipefail

# eWatts One-Click Installer
# Downloads, builds, and configures an eWatts node.
# Usage: curl -sSf https://ewatts.org/install.sh | bash

GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

echo -e "${BLUE}=============================="
echo -e "  eWatts Node Installer"
echo -e "==============================${NC}"
echo ""

# Detect OS
OS="$(uname -s)"
ARCH="$(uname -m)"
case "$OS" in
    Linux)   BINARY="ewatts-protocol-linux-$ARCH" ;;
    Darwin)  BINARY="ewatts-protocol-macos-$ARCH" ;;
    *)       echo -e "${RED}Unsupported OS: $OS${NC}"; exit 1 ;;
esac

echo -e "${BLUE}System:${NC} $OS $ARCH"
echo ""

# Check for Rust
if ! command -v cargo &> /dev/null; then
    echo -e "${YELLOW}Rust not found. Installing Rust...${NC}"
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source "$HOME/.cargo/env"
    echo -e "${GREEN}Rust installed.${NC}"
else
    echo -e "${GREEN}Rust found: $(cargo --version)${NC}"
fi

# Clone repo
INSTALL_DIR="$HOME/ewatts-protocol"
if [ -d "$INSTALL_DIR" ]; then
    echo -e "${YELLOW}Directory exists. Updating...${NC}"
    cd "$INSTALL_DIR"
    git pull
else
    echo -e "${BLUE}Cloning eWatts...${NC}"
    git clone https://github.com/4Ewatts/ewatts-protocol.git "$INSTALL_DIR"
    cd "$INSTALL_DIR"
fi

# Build
echo -e "${BLUE}Building eWatts (this may take a few minutes)...${NC}"
cargo build --release 2>&1 | tail -5

# Verify
BIN="$INSTALL_DIR/target/release/ewatts-protocol"
if [ ! -f "$BIN" ]; then
    echo -e "${RED}Build failed. Check the output above.${NC}"
    exit 1
fi

# ── Interactive configuration ──
echo ""
echo -e "${BLUE}=== Mining Configuration ===${NC}"
echo "eWatts can mine in the background while you use your computer."
echo ""

# Schedule
echo -e "Do you want to restrict mining to specific hours?"
echo -e "Examples: ${YELLOW}22:00-08:00${NC} (mine at night), ${YELLOW}09:00-18:00${NC} (mine during work), or leave empty for 24/7."
read -p "Schedule (HH:MM-HH:MM or empty): " SCHEDULE_INPUT
SCHEDULE_FLAG=""
if [ -n "$SCHEDULE_INPUT" ]; then
    SCHEDULE_FLAG="--schedule '$SCHEDULE_INPUT'"
    echo -e "  ${GREEN}Schedule set: $SCHEDULE_INPUT${NC}"
else
    echo -e "  ${GREEN}Mining 24/7${NC}"
fi

# Idle threshold
echo ""
echo "Do you want mining to pause when your CPU is busy?"
echo -e "Examples: ${YELLOW}50${NC} (pause if CPU > 50%), ${YELLOW}30${NC} (pause if CPU > 30%), or leave empty for never."
read -p "CPU threshold % (or empty): " IDLE_INPUT
IDLE_FLAG=""
if [ -n "$IDLE_INPUT" ]; then
    IDLE_FLAG="--idle-threshold $IDLE_INPUT"
    echo -e "  ${GREEN}Idle threshold set: CPU < $IDLE_INPUT%${NC}"
else
    echo -e "  ${GREEN}Always mine (no CPU check)${NC}"
fi

# Systemd service (Linux only)
SERVICE_INSTALLED=false
if [ "$OS" = "Linux" ] && command -v systemctl &> /dev/null; then
    echo ""
    echo -e "Do you want to install eWatts as a ${BLUE}background service${NC}?"
    echo "It will start automatically on boot and run in the background."
    read -p "Install as systemd service? (y/n): " SERVICE_ANS
    if [ "$SERVICE_ANS" = "y" ] || [ "$SERVICE_ANS" = "Y" ]; then
        SERVICE_FILE="$HOME/.config/systemd/user/ewatts.service"
        mkdir -p "$HOME/.config/systemd/user"
        
        # Build the ExecStart line
        START_CMD="$BIN start $SCHEDULE_FLAG $IDLE_FLAG"
        cat > "$SERVICE_FILE" << EOF
[Unit]
Description=eWatts Protocol Node
After=network.target

[Service]
ExecStart=$START_CMD
Restart=on-failure
RestartSec=10
WorkingDirectory=$INSTALL_DIR

[Install]
WantedBy=default.target
EOF
        systemctl --user daemon-reload 2>/dev/null || true
        echo -e "  ${GREEN}Service file created: $SERVICE_FILE${NC}"
        SERVICE_INSTALLED=true
    fi
fi

echo ""
echo -e "${GREEN}======================================"
echo -e "  eWatts installed successfully!"
echo -e "======================================${NC}"
echo ""
echo -e "  Binary: ${BLUE}$BIN${NC}"
echo -e "  Data:   ${BLUE}$INSTALL_DIR/ewatts_data/${NC}"
echo ""
echo -e "Quick start:"
echo -e "  ${YELLOW}cd $INSTALL_DIR${NC}"

if [ "$SERVICE_INSTALLED" = true ]; then
    echo -e "  ${YELLOW}systemctl --user start ewatts${NC}    # Start the service"
    echo -e "  ${YELLOW}systemctl --user enable ewatts${NC}   # Auto-start on boot"
    echo -e "  ${YELLOW}systemctl --user status ewatts${NC}   # Check status"
else
    echo -e "  ${YELLOW}./target/release/ewatts-protocol start $SCHEDULE_FLAG $IDLE_FLAG${NC}"
fi
echo ""
echo -e "Dashboard: http://localhost:8080/"
echo -e "Explorer:  https://ewatts.org/explorer.html"
echo -e "Docs:      https://ewatts.org"
echo ""
