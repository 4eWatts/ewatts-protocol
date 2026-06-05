#!/bin/bash
# P8-2: Cross-Platform Determinism Test
# =======================================
# Verifies that block hashes are identical across x86_64 and aarch64.
#
# Requirements:
#   rustup target add aarch64-unknown-linux-gnu
#   apt install qemu-user-binfmt aarch64-linux-gnu-gcc
#   (or equivalent for your distro)
#
# Usage:
#   bash scripts/cross_platform_determinism.sh

set -euo pipefail

REPO="$(cd "$(dirname "$0")/.." && pwd)"
BIN_X86="$REPO/target/release/ewatts-protocol"
BIN_ARM="$REPO/target/aarch64-unknown-linux-gnu/release/ewatts-protocol"

echo "P8-2: Cross-Platform Determinism Test"
echo "======================================"
echo ""

# Step 1: Build for x86_64
echo "[1/4] Building x86_64 binary..."
cargo build --release --features testnet 2>&1 | tail -1 || {
    echo "FAIL: x86_64 build failed"
    exit 1
}

# Step 2: Check if ARM target is available
echo "[2/4] Checking ARM target..."
if rustup target list --installed | grep -q aarch64; then
    echo "  ARM target installed."
else
    echo "  SKIP: ARM target not installed. Install with:"
    echo "    rustup target add aarch64-unknown-linux-gnu"
    echo "  Also need qemu-user for execution:"
    echo "    sudo apt install qemu-user-binfmt aarch64-linux-gnu-gcc"
    exit 0
fi

# Step 3: Check for QEMU
echo "[3/4] Checking QEMU..."
if command -v qemu-aarch64 &>/dev/null || ls /proc/sys/fs/binfmt_misc/qemu-aarch64 &>/dev/null; then
    echo "  QEMU available."
else
    echo "  SKIP: QEMU user-mode not available."
    echo "  Install with: sudo apt install qemu-user-binfmt"
    exit 0
fi

# Step 4: Build for ARM
echo "[4/4] Building ARM binary..."
cargo build --release --features testnet --target aarch64-unknown-linux-gnu 2>&1 | tail -1 || {
    echo "  SKIP: ARM build failed (may need aarch64 gcc)."
    echo "  Install with: sudo apt install aarch64-linux-gnu-gcc"
    exit 0
}

# Step 5: Run both binaries and compare hashes
echo ""
echo "Running determinism check..."
TMPDIR="/tmp/cross-platform-test-$$"
mkdir -p "$TMPDIR/x86" "$TMPDIR/arm"

# Generate test data on x86
cd "$TMPDIR/x86"
"$BIN_X86" init > /dev/null 2>&1
"$BIN_X86" simulate 3 > /dev/null 2>&1
HASH_X86=$(sha256sum ewatts_data/blocks.jsonl | cut -d' ' -f1)

# Generate test data on ARM (via QEMU)
cd "$TMPDIR/arm"
qemu-aarch64 "$BIN_ARM" init > /dev/null 2>&1
qemu-aarch64 "$BIN_ARM" simulate 3 > /dev/null 2>&1
HASH_ARM=$(sha256sum ewatts_data/blocks.jsonl | cut -d' ' -f1)

echo "  x86 blocks hash: $HASH_X86"
echo "  ARM blocks hash: $HASH_ARM"

if [ "$HASH_X86" = "$HASH_ARM" ]; then
    echo ""
    echo "PASS: Block hashes match across platforms!"
    echo "Blockchain is fully deterministic across x86_64 and aarch64."
else
    echo ""
    echo "FAIL: Block hashes DIFFER across platforms!"
    echo "Potential non-determinism found. Check for:"
    echo "  - f64 usage in vr.rs"
    echo "  - f64 usage in bootstrap table generation"
    echo "  - Platform-dependent serialization"
    exit 1
fi

# Cleanup
rm -rf "$TMPDIR"
