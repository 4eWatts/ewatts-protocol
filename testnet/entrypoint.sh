#!/bin/bash
set -e

# eWatts Testnet Docker Entrypoint
# Detects mode from environment and starts the node accordingly.

P2P_PORT="${P2P_PORT:-9000}"
DASH_PORT="${DASH_PORT:-8080}"
DIFFICULTY="${DIFFICULTY:-100}"

init_if_needed() {
    if [ ! -f /data/ewatts_data/utxo.json ]; then
        echo "First run - initializing genesis..."
        cd /data && ewattsd init
    fi
}

case "${MODE}" in
    bootstrap)
        init_if_needed
        echo "Starting bootstrap node on port ${P2P_PORT}..."
        exec ewattsd start \
            --p2p \
            --p2p-port "${P2P_PORT}" \
            --dash-port "${DASH_PORT}" \
            --difficulty "${DIFFICULTY}"
        ;;
    peer)
        init_if_needed
        if [ -n "${BOOTSTRAP_ADDR}" ]; then
            # Resolve bootstrap container IP via Docker DNS
            BOOTSTRAP_MADDR="${BOOTSTRAP_ADDR}"
        fi
        echo "Starting peer node, bootstrap: ${BOOTSTRAP_MADDR:-none}..."
        exec ewattsd start \
            --p2p \
            --p2p-port "${P2P_PORT}" \
            --dash-port "${DASH_PORT}" \
            --difficulty "${DIFFICULTY}" \
            ${BOOTSTRAP_MADDR:+--bootstrap "${BOOTSTRAP_MADDR}"}
        ;;
    miner)
        init_if_needed
        echo "Starting standalone miner..."
        exec ewattsd start \
            --dash-port "${DASH_PORT}" \
            --difficulty "${DIFFICULTY}"
        ;;
    *)
        echo "Unknown mode: ${MODE}"
        echo "Usage: MODE=bootstrap|peer|miner"
        exit 1
        ;;
esac
