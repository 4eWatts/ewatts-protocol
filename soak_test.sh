#!/bin/bash
# eWatts Soak Test — Day/Night mode switching
# Light mode (08-00 UTC): 4 nodes, diff 300, 2s stagger
# Heavy mode (00-08 UTC):  8 nodes, diff 1000, 1s stagger
set -uo pipefail

REPO="/home/claw/.openclaw/workspace/ewatts-protocol-repo"
BIN="$REPO/target/release/ewatts-protocol"
SOAK_DIR="/tmp/ewatts-soak"
LOG="$SOAK_DIR/soak.log"

# PID-tracked helpers (no lock — soak runs for hours, cert scripts are brief)
SKIP_LOCK=1 source "$REPO/scripts/ewatts_common.sh"

# ── Config per mode ────────────────────────────────────────────────────
# Always runs light mode to avoid restart spikes on night cycle.
MODE="${1:-light}"

if [ "$MODE" = "heavy" ]; then
  NODE_COUNT=30
  DIFFICULTY=1000
  STAGGER=1
  MODE_LABEL="HEAVY"
else
  NODE_COUNT=6
  DIFFICULTY=300
  STAGGER=2
  MODE_LABEL="LIGHT"
fi

BASE_P2P=25050
BASE_DASH=26050

# ── Cleanup previous ───────────────────────────────────────────────────
# Kill only previously tracked PIDs (or stale ones from prior runs)
kill_tracked 2>/dev/null
# Also clean any orphans from potentially aborted runs
ps aux | grep -E "target/release/ewatts-protocol" | grep -v grep | \
  awk '{print $2}' | xargs -r kill 2>/dev/null || true
sleep 1

# ── Setup directories ──────────────────────────────────────────────────
rm -rf "$SOAK_DIR" 2>/dev/null
mkdir -p "$SOAK_DIR"
for i in $(seq 0 $((NODE_COUNT - 1))); do
  mkdir -p "$SOAK_DIR/node$i/ewatts_data"
done

log() { echo "[$(date -u '+%Y-%m-%d %H:%M:%S')] [$MODE_LABEL] $*" | tee -a "$LOG"; }

# ── Init ───────────────────────────────────────────────────────────────
cd "$SOAK_DIR/node0"
$BIN init > /dev/null 2>&1
log "Node0 initialized"

for i in $(seq 1 $((NODE_COUNT - 1))); do
  cp "$SOAK_DIR/node0/ewatts_data/blocks.jsonl" "$SOAK_DIR/node$i/ewatts_data/" 2>/dev/null || true
  cp "$SOAK_DIR/node0/ewatts_data/genesis.key" "$SOAK_DIR/node$i/ewatts_data/" 2>/dev/null || true
  cp "$SOAK_DIR/node0/ewatts_data/miner.key" "$SOAK_DIR/node$i/ewatts_data/" 2>/dev/null || true
done
log "Nodes 1-$((NODE_COUNT-1)) initialized with shared genesis"

# ── Start boot ─────────────────────────────────────────────────────────
cd "$SOAK_DIR/node0"
$BIN start --p2p --p2p-port $BASE_P2P --dash-port $BASE_DASH --difficulty $DIFFICULTY \
  > "$SOAK_DIR/node0/stdout.log" 2>&1 &
N0_PID=$!
track_pid $N0_PID
log "Node0 started (PID=$N0_PID, P2P=$BASE_P2P, dash=$BASE_DASH, diff=$DIFFICULTY)"

sleep 25

PID0=$(grep -a -oP 'P2P Node ID: \K\S+' "$SOAK_DIR/node0/stdout.log" | head -1)
if [ -z "$PID0" ]; then
  log "ERROR: Could not get Node0 peer ID"; tail -20 "$SOAK_DIR/node0/stdout.log" >> "$LOG"; exit 1
fi
log "Node0 peer ID: $PID0"

# ── Start peers with stagger ──────────────────────────────────────────
declare -a PIDS
PIDS[0]=$N0_PID
for i in $(seq 1 $((NODE_COUNT - 1))); do
  sleep $STAGGER
  cd "$SOAK_DIR/node$i"
  BPORT=$((BASE_P2P + i))
  DPORT=$((BASE_DASH + i))
  $BIN start --p2p --p2p-port $BPORT --dash-port $DPORT --difficulty $DIFFICULTY \
    --bootstrap "/ip4/127.0.0.1/tcp/$BASE_P2P/p2p/$PID0" > "$SOAK_DIR/node$i/stdout.log" 2>&1 &
  PIDS[$i]=$!
  track_pid $!
  log "Node$i started (PID=${PIDS[$i]}, P2P=$BPORT, dash=$DPORT)"
done

sleep 5

# ── Metrics ────────────────────────────────────────────────────────────
METRICS="$SOAK_DIR/metrics.csv"
HEADER="timestamp,elapsed_h,mode,difficulty,nodes"
for i in $(seq 0 $((NODE_COUNT - 1))); do HEADER+=",n${i}_blocks"; done
for i in $(seq 0 $((NODE_COUNT - 1))); do HEADER+=",n${i}_mem_kb"; done
HEADER+=",cpu_pct"
echo "$HEADER" > "$METRICS"

START_TS=$(date +%s)
log "Soak test running. Mode=$MODE_LABEL Diff=$DIFFICULTY Nodes=$NODE_COUNT PIDs: ${PIDS[*]}"
log "Logs: $LOG"
log "---"

# ── Monitoring loop ────────────────────────────────────────────────────
while true; do
  NOW=$(date +%s)
  ELAPSED=$(( (NOW - START_TS) / 3600 ))
  TS=$(date -u '+%Y-%m-%d %H:%M:%S')

  LINE="$TS,$ELAPSED,$MODE_LABEL,$DIFFICULTY,$NODE_COUNT"
  for i in $(seq 0 $((NODE_COUNT - 1))); do
    B=$(wc -l < "$SOAK_DIR/node$i/ewatts_data/blocks.jsonl" 2>/dev/null || echo "0")
    LINE+=",$B"
  done

  TOTAL_CPU=0
  for i in $(seq 0 $((NODE_COUNT - 1))); do
    PID=${PIDS[$i]}
    MEM=$(ps -o rss= -p $PID 2>/dev/null | tr -d ' ' || echo "0")
    LINE+=",$MEM"
    CPU=$(ps -o %cpu= -p $PID 2>/dev/null | tr -d ' ' || echo "0")
    TOTAL_CPU=$(echo "$TOTAL_CPU + $CPU" | bc 2>/dev/null || echo "0")
  done
  LINE+=",$TOTAL_CPU"
  echo "$LINE" >> "$METRICS"

  # Mode switching disabled — runs continuously without restart

  # Health check
  for i in $(seq 0 $((NODE_COUNT - 1))); do
    PID=${PIDS[$i]}
    if ! kill -0 $PID 2>/dev/null; then
      log "WARN: Node$i died! Restarting..."
      cd "$SOAK_DIR/node$i"
      BPORT=$((BASE_P2P + i))
      DPORT=$((BASE_DASH + i))
      if [ "$i" -eq 0 ]; then
        $BIN start --p2p --p2p-port $BPORT --dash-port $DPORT --difficulty $DIFFICULTY \
          > "$SOAK_DIR/node$i/stdout.log" 2>&1 &
      else
        $BIN start --p2p --p2p-port $BPORT --dash-port $DPORT --difficulty $DIFFICULTY \
          --bootstrap "/ip4/127.0.0.1/tcp/$BASE_P2P/p2p/$PID0" > "$SOAK_DIR/node$i/stdout.log" 2>&1 &
      fi
      PIDS[$i]=$!
      track_pid $!
      log "Node$i restarted (PID=${PIDS[$i]})"
    fi
  done

  sleep 600
done
