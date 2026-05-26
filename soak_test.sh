#!/bin/bash
# eWatts Phase 5 — 72h Soak Test Harness
# Runs 2 P2P nodes, collects metrics, reports via log.
set -uo pipefail

REPO="/home/claw/.openclaw/workspace/ewatts-protocol-repo"
BIN="$REPO/target/release/ewatts-protocol"
SOAK_DIR="/tmp/ewatts-soak"
LOG="$SOAK_DIR/soak.log"
METRICS="$SOAK_DIR/metrics.csv"
NODE0_DIR="$SOAK_DIR/node0"
NODE1_DIR="$SOAK_DIR/node1"
NODE0_PORT=25050
NODE1_PORT=25051
NODE0_DASH=26050
NODE1_DASH=26051

mkdir -p "$NODE0_DIR/ewatts_data" "$NODE1_DIR/ewatts_data"

log() { echo "[$(date -u '+%Y-%m-%d %H:%M:%S')] $*" | tee -a "$LOG"; }

ewatts_kill() {
  ps aux | grep -E "target/release/ewatts-protocol" | grep -v grep | \
    awk '{print $2}' | xargs -r kill "$@" 2>/dev/null || true
  sleep 1
}

# ── Init ──────────────────────────────────────────────────────────────
ewatts_kill

# Boot node (node0)
cd "$NODE0_DIR"
$BIN init > /dev/null 2>&1
log "Node0 initialized"

# Peer node (node1) — copy genesis from boot
cp "$NODE0_DIR/ewatts_data/blocks.jsonl" "$NODE1_DIR/ewatts_data/" 2>/dev/null || true
cp "$NODE0_DIR/ewatts_data/genesis.key" "$NODE1_DIR/ewatts_data/" 2>/dev/null || true
cp "$NODE0_DIR/ewatts_data/miner.key" "$NODE1_DIR/ewatts_data/" 2>/dev/null || true
log "Node1 initialized with shared genesis"

# ── Start ─────────────────────────────────────────────────────────────

# Node0 (boot, mines)
cd "$NODE0_DIR"
$BIN start --p2p --p2p-port $NODE0_PORT --dash-port $NODE0_DASH --difficulty 100 > "$NODE0_DIR/stdout.log" 2>&1 &
N0_PID=$!
log "Node0 started (PID=$N0_PID, P2P=$NODE0_PORT, dash=$NODE0_DASH)"

sleep 15

# Get boot peer ID for node1 bootstrap
PID0=$(grep -oP 'P2P Node ID: \K\S+' "$NODE0_DIR/stdout.log" | head -1)
if [ -z "$PID0" ]; then
  log "ERROR: Could not get Node0 peer ID"
  tail -20 "$NODE0_DIR/stdout.log" >> "$LOG"
  exit 1
fi
log "Node0 peer ID: $PID0"

# Node1 (peer, mines)
cd "$NODE1_DIR"
$BIN start --p2p --p2p-port $NODE1_PORT --dash-port $NODE1_DASH --difficulty 100 \
  --bootstrap "/ip4/127.0.0.1/tcp/$NODE0_PORT/p2p/$PID0" > "$NODE1_DIR/stdout.log" 2>&1 &
N1_PID=$!
log "Node1 started (PID=$N1_PID, P2P=$NODE1_PORT, dash=$NODE1_DASH)"

sleep 10

# ── Metrics header ────────────────────────────────────────────────────
if [ ! -f "$METRICS" ]; then
  echo "timestamp,elapsed_h,n0_blocks,n1_blocks,n0_mem_kb,n1_mem_kb,cpu_pct" > "$METRICS"
fi

START_TS=$(date +%s)

# ── Monitoring loop ────────────────────────────────────────────────────
log "Soak test running. PID0=$N0_PID PID1=$N1_PID"
log "Metrics: $METRICS"
log "Logs: $LOG"
log "---"

while true; do
  NOW=$(date +%s)
  ELAPSED=$(( (NOW - START_TS) / 3600 ))
  TIMESTAMP=$(date -u '+%Y-%m-%d %H:%M:%S')

  N0_BLOCKS=$(wc -l < "$NODE0_DIR/ewatts_data/blocks.jsonl" 2>/dev/null || echo "0")
  N1_BLOCKS=$(wc -l < "$NODE1_DIR/ewatts_data/blocks.jsonl" 2>/dev/null || echo "0")
  N0_MEM=$(ps -o rss= -p $N0_PID 2>/dev/null | tr -d ' ' || echo "0")
  N1_MEM=$(ps -o rss= -p $N1_PID 2>/dev/null | tr -d ' ' || echo "0")

  # CPU usage (average of both nodes)
  N0_CPU=$(ps -o %cpu= -p $N0_PID 2>/dev/null | tr -d ' ' || echo "0")
  N1_CPU=$(ps -o %cpu= -p $N1_PID 2>/dev/null | tr -d ' ' || echo "0")
  CPU_PCT=$(echo "$N0_CPU + $N1_CPU" | bc 2>/dev/null || echo "0")

  echo "$TIMESTAMP,$ELAPSED,$N0_BLOCKS,$N1_BLOCKS,$N0_MEM,$N1_MEM,$CPU_PCT" >> "$METRICS"

  # Health check: if either node died, report and restart
  if ! kill -0 $N0_PID 2>/dev/null; then
    log "WARN: Node0 died! Restarting..."
    cd "$NODE0_DIR"
    $BIN start --p2p --p2p-port $NODE0_PORT --dash-port $NODE0_DASH --difficulty 100 > "$NODE0_DIR/stdout.log" 2>&1 &
    N0_PID=$!
    log "Node0 restarted (PID=$N0_PID)"
  fi
  if ! kill -0 $N1_PID 2>/dev/null; then
    log "WARN: Node1 died! Restarting..."
    cd "$NODE1_DIR"
    $BIN start --p2p --p2p-port $NODE1_PORT --dash-port $NODE1_DASH --difficulty 100 \
      --bootstrap "/ip4/127.0.0.1/tcp/$NODE0_PORT/p2p/$PID0" > "$NODE1_DIR/stdout.log" 2>&1 &
    N1_PID=$!
    log "Node1 restarted (PID=$N1_PID)"
  fi

  # Status summary every hour
  if [ $((ELAPSED % 6)) -eq 0 ] && [ $ELAPSED -gt 0 ]; then
    MEM_TOTAL=$((N0_MEM + N1_MEM))
    log "Hour $ELAPSED — blocks: n0=$N0_BLOCKS n1=$N1_BLOCKS — mem: ${MEM_TOTAL}KB — cpu: ${CPU_PCT}%"
  fi

  sleep 600  # 10 minutes between samples
done
