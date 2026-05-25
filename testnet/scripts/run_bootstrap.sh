#!/bin/bash
# eWatts Testnet Bootstrap Node
#
# Run this on a machine with a public IP to serve as entry point for peers.
# Records Peer ID to p2p_peer_id.txt for sharing with peers.

set -e

BIN="${1:-./target/release/ewatts-protocol}"
P2P_PORT="${2:-9000}"
DASH_PORT="${3:-8080}"
DIFFICULTY="${4:-100}"
DATA_DIR="${5:-ewatts_data}"

if [ ! -f "${BIN}" ]; then
    echo "Binary not found: ${BIN}"
    echo "Usage: $0 [binary_path] [p2p_port] [dash_port] [difficulty] [data_dir]"
    exit 1
fi

cd "$(dirname "$0")/../.."

# Initialize if needed
if [ ! -f "${DATA_DIR}/utxo.json" ]; then
    echo "Initializing genesis..."
    "${BIN}" init
fi

# Save peer ID for sharing
PEER_ID_FILE="p2p_peer_id.txt"

echo "=== eWatts Bootstrap Node ==="
echo "Binary:    ${BIN}"
echo "P2P port:  ${P2P_PORT}"
echo "Dashboard: http://localhost:${DASH_PORT}"
echo "Data dir:  ${DATA_DIR}"
echo ""

# Start the node
"${BIN}" start \
    --p2p \
    --p2p-port "${P2P_PORT}" \
    --dash-port "${DASH_PORT}" \
    --difficulty "${DIFFICULTY}"
