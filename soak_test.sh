#!/bin/bash
# eWatts Phase 5 — 72h Soak Test Harness
# Runs 3 P2P nodes, collects metrics, reports via log.
set -uo pipefail

REPO="/home/claw/.openclaw/workspace/ewatts-protocol-repo"
BIN="$REPO/target/release/ewatts-protocol"
SOAK_DIR="/tmp/ewatts-soak"
LOG="$SOAK_DIR/soak.log"
METRICS="$SOAK_DIR/metrics.csv"

DIFFICULTY=500

# 3 nodes
NODES=(
  "0:25050:26050"
  "1:25051:26051"
  "2:25052:26052"
)

mkdir -p "$SOAK_DIR"
for entry in "${NODES[@]}"; do
  IFS=':' read -r id p2p_port dash_port <<< "$entry"
  mkdir -p "$SOAK_DIR/node$id/ewatts_data"
done

log() { echo "[$(date -u '+%Y-%m-%d %H:%M:%S')] $*" | tee -a "$LOG"; }

ewatts_kill() {
  ps aux | grep -E "target/release/ewatts-protocol" | grep -v grep | \
    awk '{print $2}' | xargs -r kill "$@" 2>/dev/null || true
  sleep 1
}

# ── Init ──────────────────────────────────────────────────────────────
ewatts_kill

# Boot node (node0)
cd "$SOAK_DIR/node0"
$BIN init > /dev/null 2>&1
log "Node0 initialized"

# Peer nodes — copy genesis from boot
for i in 1 2; do
  cp "$SOAK_DIR/node0/ewatts_data/blocks.jsonl" "$SOAK_DIR/node$i/ewatts_data/" 2>/dev/null || true
  cp "$SOAK_DIR/node0/ewatts_data/genesis.key" "$SOAK_DIR/node$i/ewatts_data/" 2>/dev/null || true
  cp "$SOAK_DIR/node0/ewatts_data/miner.key" "$SOAK_DIR/node$i/ewatts_data/" 2>/dev/null || true
done
log "Nodes 1-2 initialized with shared genesis"

# ── Start nodes ───────────────────────────────────────────────────────

# Node0 (boot, mines)
cd "$SOAK_DIR/node0"
$BIN start --p2p --p2p-port 25050 --dash-port 26050 --difficulty $DIFFICULTY > "$SOAK_DIR/node0/stdout.log" 2>&1 &
N0_PID=$!
log "Node0 started (PID=$N0_PID, P2P=25050, dash=26050, diff=$DIFFICULTY)"

sleep 20

# Get boot peer ID
PID0=$(grep -oP 'P2P Node ID: \K\S+' "$SOAK_DIR/node0/stdout.log" | head -1)
if [ -z "$PID0" ]; then
  log "ERROR: Could not get Node0 peer ID"
  tail -20 "$SOAK_DIR/node0/stdout.log" >> "$LOG"
  exit 1
fi
log "Node0 peer ID: $PID0"

# Nodes 1-2 (peers, mine)
for i in 1 2; do
  cd "$SOAK_DIR/node$i"
  BPORT=$((25050 + i))
  DPORT=$((26050 + i))
  $BIN start --p2p --p2p-port $BPORT --dash-port $DPORT --difficulty $DIFFICULTY \
    --bootstrap "/ip4/127.0.0.1/tcp/25050/p2p/$PID0" > "$SOAK_DIR/node$i/stdout.log" 2>&1 &
  eval "N${i}_PID=\$!"
  log "Node$i started (PID=$(eval echo \$N${i}_PID), P2P=$BPORT, dash=$DPORT)"
done

sleep 10

# ── Metrics header ────────────────────────────────────────────────────
if [ ! -f "$METRICS" ]; then
  echo "timestamp,elapsed_h,n0_blocks,n1_blocks,n2_blocks,n0_mem_kb,n1_mem_kb,n2_mem_kb,cpu_pct" > "$METRICS"
fi

START_TS=$(date +%s)

# PID array for easier iteration
PIDS=($N0_PID $N1_PID $N2_PID)

# ── Monitoring loop ────────────────────────────────────────────────────
log "Soak test running. PIDs: ${PIDS[*]}"
log "Metrics: $METRICS"
log "Logs: $LOG"
log "---"

while true; do
  NOW=$(date +%s)
  ELAPSED=$(( (NOW - START_TS) / 3600 ))
  TIMESTAMP=$(date -u '+%Y-%m-%d %H:%M:%S')

  BLOCKS=()
  MEMS=()
  CPUS=()
  TOTAL_MEM=0
  TOTAL_CPU=0

  for i in 0 1 2; do
    B=$(wc -l < "$SOAK_DIR/node$i/ewatts_data/blocks.jsonl" 2>/dev/null || echo "0")
    BLOCKS+=("$B")

    PID=${PIDS[$i]}
    MEM=$(ps -o rss= -p $PID 2>/dev/null | tr -d ' ' || echo "0")
    MEMS+=("$MEM")
    TOTAL_MEM=$((TOTAL_MEM + MEM))

    CPU=$(ps -o %cpu= -p $PID 2>/dev/null | tr -d ' ' || echo "0")
    CPUS+=("$CPU")
    TOTAL_CPU=$(echo "$TOTAL_CPU + $CPU" | bc 2>/dev/null || echo "0")
  done

  echo "$TIMESTAMP,$ELAPSED,${BLOCKS[0]},${BLOCKS[1]},${BLOCKS[2]},${MEMS[0]},${MEMS[1]},${MEMS[2]},$TOTAL_CPU" >> "$METRICS"

  # Health check: restart any dead node
  for i in 0 1 2; do
    PID=${PIDS[$i]}
    if ! kill -0 $PID 2>/dev/null; then
      log "WARN: Node$i died (PID=$PID)! Restarting..."
      cd "$SOAK_DIR/node$i"
      BPORT=$((25050 + i))
      DPORT=$((26050 + i))
      if [ "$i" -eq 0 ]; then
        $BIN start --p2p --p2p-port $BPORT --dash-port $DPORT --difficulty $DIFFICULTY > "$SOAK_DIR/node$i/stdout.log" 2>&1 &
      else
        $BIN start --p2p --p2p-port $BPORT --dash-port $DPORT --difficulty $DIFFICULTY \
          --bootstrap "/ip4/127.0.0.1/tcp/25050/p2p/$PID0" > "$SOAK_DIR/node$i/stdout.log" 2>&1 &
      fi
      PIDS[$i]=$!
      log "Node$i restarted (PID=${PIDS[$i]})"
    fi
  done

  # Report every 6 hours
  if [ $((ELAPSED % 6)) -eq 0 ] && [ $ELAPSED -gt 0 ] && [ "$(date +%M)" -lt "5" ]; then
    log "Hour $ELAPSED — blocks: ${BLOCKS[*]} — mem: ${TOTAL_MEM}KB — cpu: ${TOTAL_CPU}%"
  fi

  sleep 600  # 10 minutes between samples
done
