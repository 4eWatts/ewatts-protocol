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
echo -e "  ${YELLOW}./target/release/ewatts-protocol init${NC}"
echo -e "  ${YELLOW}./target/release/ewatts-protocol start${NC}"
echo ""
echo -e "Dashboard: http://localhost:8080/"
echo -e "Explorer:  https://ewatts.org/explorer.html"
echo -e "Docs:      https://ewatts.org"
echo ""
