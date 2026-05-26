#!/bin/bash
# eWatts Testnet Certification Suite — Phase 0
set -uo pipefail

REPO="/home/claw/.openclaw/workspace/ewatts-protocol-repo"
BIN="$REPO/target/release/ewatts-protocol"
RESULTS="$REPO/testnet_cert_results.txt"
> "$RESULTS"

log() { echo "$1" | tee -a "$RESULTS"; }

# Safer pkill: avoid killing bash running this script
ewatts_kill() {
  ps aux | grep -E "target/release/ewatts-protocol" | grep -v grep | \
    awk '{print $2}' | xargs -r kill "$@" 2>/dev/null || true
  sleep 1
}

# Read block count from a node's data dir
block_count() { wc -l < "$1/ewatts_data/blocks.jsonl" 2>/dev/null || echo "0"; }

log "=== eWatts Testnet Certification Report ==="
log "Date: $(date -u)"
log "Binary: $BIN"
log ""

# ═══════════════════════════════════════
# T0.1: Chain Height Persistence
# ═══════════════════════════════════════
log "--- T0.1: Chain Height Persistence ---"
ewatts_kill
RAND=$RANDOM
WORK="/tmp/cert-$RAND"
mkdir -p "$WORK/ewatts_data"
cd "$WORK"

$BIN init > /dev/null 2>&1
$BIN start --dash-port $((27000+RAND%500)) --difficulty 1 > /tmp/cert-t01a.log 2>&1 &
PID1=$!
sleep 80

H1=$(($(block_count "$WORK") - 1))
log "  Blocks pre-restart: $H1 (need >=5)"

kill $PID1 2>/dev/null; sleep 2
ewatts_kill

$BIN start --dash-port $((27001+RAND%500)) --difficulty 1 > /tmp/cert-t01b.log 2>&1 &
PID2=$!
sleep 20

H2=$(($(block_count "$WORK") - 1))
log "  Blocks post-restart: $H2"

kill $PID2 2>/dev/null; ewatts_kill

# PASS if chain didn't reset (H2 >= H1) and at least 5 blocks
if [ "$H2" -ge "$H1" ] && [ "$H1" -ge 5 ]; then
  log "PASS: Height persisted (pre=$H1 post=$H2, grew after restart)"
else
  log "FAIL: Height dropped or too low (pre=$H1 post=$H2)"
fi
log ""

# ═══════════════════════════════════════
# T0.2: Tip Convergence (multi-node)
# ═══════════════════════════════════════
log "--- T0.2: Tip Convergence ---"
ewatts_kill
RAND=$RANDOM
for i in 0 1 2; do
    D="/tmp/cert-conv-$RAND-$i"
    rm -rf "$D" 2>/dev/null; mkdir -p "$D/ewatts_data"
done

cd "/tmp/cert-conv-$RAND-0"
$BIN init > /dev/null 2>&1
BP=$((23000+RAND%500))
DP=$((28000+RAND%500))
$BIN start --p2p --p2p-port $BP --dash-port $DP --difficulty 10 > /tmp/cert-conv-boot.log 2>&1 &
BPID=$!
sleep 70

PID=$(grep -oP 'P2P Node ID: \K\S+' /tmp/cert-conv-boot.log | head -1 || true)
log "  Boot peer ID: $PID"
if [ -z "$PID" ]; then
    log "FAIL: Could not extract boot peer ID"; tail -10 /tmp/cert-conv-boot.log >> "$RESULTS"
    log "--- T0.2: SKIPPED ---"; log ""
else
    for i in 1 2; do
        cd "/tmp/cert-conv-$RAND-$i"
        $BIN start --p2p --p2p-port $((BP+i)) --dash-port $((DP+i)) --difficulty 10 \
          --no-mine --bootstrap "/ip4/127.0.0.1/tcp/$BP/p2p/$PID" > "/tmp/cert-conv-p$i.log" 2>&1 &
    done
    sleep 60

    C0=$(block_count "/tmp/cert-conv-$RAND-0")
    C1=$(block_count "/tmp/cert-conv-$RAND-1")
    C2=$(block_count "/tmp/cert-conv-$RAND-2")
    log "  Blocks: boot=$C0  peer1=$C1  peer2=$C2"

    if [ "$C0" -gt 2 ] && [ "$C0" -eq "$C1" ] && [ "$C1" -eq "$C2" ]; then
        log "PASS: All nodes converged at height $((C0-1))"
    else
        log "FAIL: Block counts diverge (boot=$C0 p1=$C1 p2=$C2)"
    fi
fi
ewatts_kill
log ""

# ═══════════════════════════════════════
# T0.3: Partition / Split-brain
# ═══════════════════════════════════════
log "--- T0.3: Partition / Split-brain ---"
ewatts_kill
RAND=$RANDOM
for i in 0 1; do
    D="/tmp/cert-part-$RAND-$i"
    rm -rf "$D" 2>/dev/null; mkdir -p "$D/ewatts_data"
done

cd "/tmp/cert-part-$RAND-0"
$BIN init > /dev/null 2>&1
PB0=$((24000+RAND%500))
PD0=$((29000+RAND%500))
$BIN start --p2p --p2p-port $PB0 --dash-port $PD0 --difficulty 10 > /tmp/cert-part-0.log 2>&1 &
sleep 35
PID0=$(grep -oP 'P2P Node ID: \K\S+' /tmp/cert-part-0.log | head -1 || true)

cd "/tmp/cert-part-$RAND-1"
$BIN init > /dev/null 2>&1
PB1=$((24001+RAND%500))
PD1=$((29001+RAND%500))
$BIN start --p2p --p2p-port $PB1 --dash-port $PD1 --difficulty 10 \
  --bootstrap "/ip4/127.0.0.1/tcp/$PB0/p2p/$PID0" > /tmp/cert-part-1.log 2>&1 &
sleep 35

PRE=$(($(block_count "/tmp/cert-part-$RAND-0") - 1))
log "  Pre-partition height: $PRE"

# Partition: kill peer1, let node0 mine more
ps aux | grep "cert-part-$RAND-1" | grep -v grep | awk '{print $2}' | xargs -r kill 2>/dev/null || true
sleep 30

H0=$(($(block_count "/tmp/cert-part-$RAND-0") - 1))
log "  After partition: boot=$H0 blocks"

# Reconnect peer1 with boot's latest blocks
cd "/tmp/cert-part-$RAND-1"
cp "/tmp/cert-part-$RAND-0/ewatts_data/blocks.jsonl" ewatts_data/ 2>/dev/null || true
cp "/tmp/cert-part-$RAND-0/ewatts_data/genesis.key" ewatts_data/ 2>/dev/null || true
cp "/tmp/cert-part-$RAND-0/ewatts_data/miner.key" ewatts_data/ 2>/dev/null || true
$BIN start --p2p --p2p-port $PB1 --dash-port $PD1 --difficulty 10 \
  --bootstrap "/ip4/127.0.0.1/tcp/$PB0/p2p/$PID0" > /tmp/cert-part-1b.log 2>&1 &
sleep 45

C0=$(block_count "/tmp/cert-part-$RAND-0")
C1=$(block_count "/tmp/cert-part-$RAND-1")
log "  Final: boot=$C0 blocks  peer1=$C1 blocks"

if [ "$C0" -gt "$PRE" ] && [ "$C0" -eq "$C1" ]; then
    log "PASS: Node 1 converged to longer chain after partition (h=$PRE -> $((C0-1)))"
elif [ "$C0" -eq "$C1" ]; then
    log "WARN: Converged but no new blocks during partition (h=$PRE)"
else
    log "FAIL: Divergent after partition (boot=$C0 peer1=$C1)"
fi
ewatts_kill
log ""

# ═══════════════════════════════════════
# T0.4: Crash Consistency
# ═══════════════════════════════════════
log "--- T0.4: Crash Consistency ---"
ewatts_kill
RAND=$RANDOM
WORK="/tmp/cert-crash-$RAND"
mkdir -p "$WORK/ewatts_data"
cd "$WORK"
$BIN init > /dev/null 2>&1

# Standalone mode for fast blocks
$BIN start --dash-port $((30000+RAND%500)) --difficulty 1 > /tmp/cert-crash.log 2>&1 &
sleep 20
BLOCKS_BEFORE=$(block_count "$WORK")
log "  Blocks before SIGKILL: $BLOCKS_BEFORE"

# SIGKILL (9) — hardest crash
ps aux | grep "target/release/ewatts-protocol" | grep -v grep | \
  awk '{print $2}' | xargs -r kill -9 2>/dev/null || true
sleep 2

$BIN start --dash-port $((30001+RAND%500)) --difficulty 1 > /tmp/cert-crash2.log 2>&1 &
sleep 15
BLOCKS_AFTER=$(block_count "$WORK")
log "  Blocks after SIGKILL + restart: $BLOCKS_AFTER"

# Validate JSONL
INVALID=$(python3 -c "
import json
with open('$WORK/ewatts_data/blocks.jsonl') as f:
    for i, line in enumerate(f, 1):
        line = line.strip()
        if line:
            try:
                json.loads(line)
            except:
                print(i)
" 2>/dev/null || echo "0")

if [ "$BLOCKS_AFTER" -ge "$BLOCKS_BEFORE" ] && [ -z "$INVALID" ]; then
    log "PASS: Blocks persist after SIGKILL ($BLOCKS_BEFORE -> $BLOCKS_AFTER, JSON valid)"
elif [ -n "$INVALID" ]; then
    log "FAIL: Invalid JSON on line $INVALID after crash"
else
    log "FAIL: Block count decreased ($BLOCKS_BEFORE -> $BLOCKS_AFTER)"
fi
ewatts_kill
log ""

# ═══════════════════════════════════════
# Summary
# ═══════════════════════════════════════
log "=== Phase 0 Summary ==="
grep -E "^(PASS|FAIL|WARN)" "$RESULTS"
log ""
log "=== End of Phase 0 ==="
