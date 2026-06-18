#!/bin/bash
# eWatts Testnet Peer Node
#
# Connects to a bootstrap node and starts mining.

set -e

BIN="${1:-./target/release/ewatts-protocol}"
BOOTSTRAP_PEER="${2}"
P2P_PORT="${3:-9001}"
DASH_PORT="${4:-8081}"
DIFFICULTY="${5:-100}"
DATA_DIR="${6:-ewatts_data}"

if [ ! -f "${BIN}" ]; then
    echo "Binary not found: ${BIN}"
    echo "Usage: $0 <binary_path> <bootstrap_multiaddr> [p2p_port] [dash_port] [difficulty] [data_dir]"
    echo ""
    echo "Example bootstrap multiaddr:"
    echo "  /ip4/203.0.113.10/tcp/9000/p2c/12D3KooW..."
    exit 1
fi

if [ -z "${BOOTSTRAP_PEER}" ]; then
    echo "Error: bootstrap multiaddr is required."
    echo "Usage: $0 <binary_path> <bootstrap_multiaddr> [p2p_port] [dash_port] [difficulty] [data_dir]"
    echo ""
    echo "Get the bootstrap multiaddr from the bootstrap node's logs."
    echo "It looks like: /ip4/<IP>/tcp/<PORT>/p2p/<PEER_ID>"
    exit 1
fi

cd "$(dirname "$0")/../.."

# Copy data from bootstrap if needed (for same genesis)
if [ ! -f "${DATA_DIR}/utxo.json" ]; then
    echo "Run 'init' before starting a peer, or copy the bootstrap's ewatts_data/."
    echo "For now, running init with a fresh genesis..."
    "${BIN}" init
    echo ""
    echo "WARNING: Fresh genesis generates a new random key."
    echo "Peers will mine on different chains unless they share genesis."
    echo "For a shared testnet, copy ewatts_data/ from the bootstrap node."
fi

echo "=== eWatts Peer Node ==="
echo "Binary:    ${BIN}"
echo "Bootstrap: ${BOOTSTRAP_PEER}"
echo "P2P port:  ${P2P_PORT}"
echo "Dashboard: http://localhost:${DASH_PORT}"
echo ""

"${BIN}" start \
    --p2p \
    --p2p-port "${P2P_PORT}" \
    --dash-port "${DASH_PORT}" \
    --difficulty "${DIFFICULTY}" \
    --bootstrap "${BOOTSTRAP_PEER}"
