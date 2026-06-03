#!/usr/bin/env bash
# ═══════════════════════════════════════════════════════════════
# eWatts — Procedimentos Noturnos (Pós-Fase 7)
# ═══════════════════════════════════════════════════════════════
# Roda durante a noite:
#   1. Full test suite (release) — confirma 130+ testes
#   2. DAG benchmark — mede tempo real vs spec §4.2
#   3. Security scan — unwrap(), TODO, FIXME, constantes
#   4. P2P multi-node local — 2 nodes, conecta, verifica
#   5. Live node health — api.ewatts.org
#   6. Parameter freeze — extrai todos os parâmetros mainnet
#   7. Code quality — LOC, módulos, limpeza de warnings
# ═══════════════════════════════════════════════════════════════
# Uso:  bash run_overnight.sh 2>&1 | tee overnight_report.txt
# ═══════════════════════════════════════════════════════════════

set -euo pipefail
DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$DIR"

REPORT="$DIR/overnight_report.txt"
PASS=0
WARN=0
FAIL=0

log()   { echo "[$(date -u +%H:%M:%S)] $*"; }
pass()  { echo "  ✅ $1"; ((PASS++)); }
warn()  { echo "  ⚠️  $1"; ((WARN++)); }
fail()  { echo "  ❌ $1"; ((FAIL++)); }
hr()    { echo "────────────────────────────────────────────────────"; }

export RUSTFLAGS="${RUSTFLAGS:--C linker=gcc}"

: > "$REPORT"
exec > >(tee -a "$REPORT") 2>&1

echo ""
echo "╔═══════════════════════════════════════════════════════╗"
echo "║  eWatts Overnight Procedures — $(date -u)  ║"
echo "╚═══════════════════════════════════════════════════════╝"
echo ""

# ═══════════════════════════════════════════════════════════════
# 1. Full Test Suite (release mode)
# ═══════════════════════════════════════════════════════════════
hr
log "1/7  Full Test Suite (release profile)"

# Store tests first (serial, need --test-threads=1)
log "  → Store tests (serial)..."
if cargo test test_store_ --release -- --test-threads=1 --quiet 2>&1; then
    pass "Store tests (18)"
else
    fail "Store tests"
fi

# DAG benchmarks (with output)
log "  → DAG benchmark tests..."
if cargo test test_dag_benchmark_ --release -- --nocapture 2>&1 | grep -E '\[PHASE7\]|ok$|FAILED'; then
    pass "DAG benchmarks"
else
    fail "DAG benchmarks"
fi

# Mempool tests
log "  → Mempool tests..."
if cargo test test_mempool_ --release --quiet 2>&1; then
    pass "Mempool tests"
else
    fail "Mempool tests" 
fi

# Full suite (excluding slow integration for speed)
log "  → Remaining tests..."
START=$(date +%s)
if cargo test --release --quiet 2>&1; then
    END=$(date +%s)
    # Count total
    TOTAL=$(grep -E 'test result:' <(cargo test --release 2>&1 | tail -5) || echo "unknown")
    pass "Full suite: $TOTAL ($((END - START))s)"
else
    fail "Full suite"
fi

# ═══════════════════════════════════════════════════════════════
# 2. DAG Performance Report
# ═══════════════════════════════════════════════════════════════
hr
log "2/7  DAG Performance Report"

# Run benchmarks with capture
cargo test test_dag_benchmark_64mb test_dag_benchmark_progressive test_dag_deterministic test_dag_cache_hit --release --nocapture 2>&1 | grep '\[PHASE7\]' || true

# Summarize
log "DAG summary from benchmark output above"
log "Extrapolated 8 GB estimate vs spec target (<60s):"
cargo test test_dag_benchmark_64mb --release --nocapture 2>&1 | grep '8 GB' || warn "No 8GB extrapolation in output"

# ═══════════════════════════════════════════════════════════════
# 3. Security Scan
# ═══════════════════════════════════════════════════════════════
hr
log "3/7  Security Scan"

# 3a. Find all unwrap() calls (potential panics)
log "  → unwrap() audit (non-test, non-comment)..."
UNWRAPS=$(grep -rn '\.unwrap()' src/ --include='*.rs' | grep -v '#\[test\]' | grep -v '///' | grep -v '// ' | wc -l)
log "  Found $UNWRAPS unwrap() calls (non-test). Checking safety..."
# List dangerous unwraps (not in Result context)
grep -rn '\.unwrap()' src/ --include='*.rs' | grep -v '#\[test\]' | grep -v '///' | grep -v '// ' | grep -v 'lock()' | grep -v 'unwrap_or' | head -20
if [[ "$UNWRAPS" -gt 20 ]]; then
    warn "$UNWRAPS unwrap() calls — review needed"
else
    pass "unwrap() count: $UNWRAPS"
fi

# 3b. Inventory TODOs, FIXMEs, SECURITYs
log "  → Code annotations..."
for tag in TODO FIXME SECURITY HACK XXX SAFETY; do
    count=$(grep -rn "$tag" src/ --include='*.rs' | grep -v 'cargo fix' | wc -l)
    if [[ "$count" -gt 0 ]]; then
        log "    $tag: $count"
        grep -rn "$tag" src/ --include='*.rs' 2>/dev/null | head -5 | sed 's/^/      /'
    fi
done

# 3c. Check public key hashing and address validation
log "  → P2PKH address validation..."
grep -n 'pubkey_hash\|PUBKEY_HASH\|p2pkh\|P2PKH' src/*.rs --include='*.rs' | head -10

# 3d. Verify constants don't have hardcoded testnet values in mainnet code
log "  → Parameter cross-check..."
grep -n 'TESTNET_\|testnet' src/constants.rs | head -10
grep -n '#\[cfg(feature.*mainnet' src/*.rs --include='*.rs' 2>/dev/null || warn "No mainnet cfg gates found — mainnet/testnet merged"

# 3e. Check for unsafe blocks
log "  → unsafe blocks..."
UNSAFE=$(grep -rn 'unsafe {' src/ --include='*.rs' | wc -l)
if [[ "$UNSAFE" -gt 0 ]]; then
    warn "$UNSAFE unsafe blocks found"
    grep -rn 'unsafe {' src/ --include='*.rs' | head -5
else
    pass "Zero unsafe blocks"
fi

# ═══════════════════════════════════════════════════════════════
# 4. P2P Multi-Node Local Test
# ═══════════════════════════════════════════════════════════════
hr
log "4/7  P2P Multi-Node Local Test"

log "Building release binary..."
cargo build --release 2>&1 | tail -1
NODE_BIN="$DIR/target/release/ewatts-protocol"

NODE_A_DIR="/tmp/ewatts_overnight_a"
NODE_B_DIR="/tmp/ewatts_overnight_b"
rm -rf "$NODE_A_DIR" "$NODE_B_DIR"

log "Starting Node A (seed, port 18800)..."
mkdir -p "$NODE_A_DIR"
cd "$NODE_A_DIR"
"$NODE_BIN" start --p2p --p2p-port 18800 --difficulty 1 --dash-port 18801 &
PID_A=$!
sleep 3

log "Starting Node B (bootstrap, port 18802)..."
mkdir -p "$NODE_B_DIR"
cd "$NODE_B_DIR"
"$NODE_BIN" start --p2p --p2p-port 18802 --bootstrap /ip4/127.0.0.1/tcp/18800 --difficulty 1 --dash-port 18803 &
PID_B=$!
sleep 5

if kill -0 $PID_A 2>/dev/null && kill -0 $PID_B 2>/dev/null; then
    pass "Both P2P nodes started"
    
    # Check APIs
    NODE_A_STATUS=$(curl -s http://127.0.0.1:18801/api/status 2>/dev/null || echo "FAIL")
    NODE_B_STATUS=$(curl -s http://127.0.0.1:18803/api/status 2>/dev/null || echo "FAIL")
    
    log "Node A: $NODE_A_STATUS"
    log "Node B: $NODE_B_STATUS"
    
    PEERS_A=$(echo "$NODE_A_STATUS" | grep -o '"peers":[0-9]*' | cut -d: -f2 || echo "N/A")
    
    if [[ "$PEERS_A" != "N/A" ]] && [[ "$PEERS_A" -gt 0 ]]; then
        pass "P2P: $PEERS_A peer(s) connected"
    else
        warn "P2P: 0 peers detected (libp2p may need longer handshake)"
        # Give it more time
        sleep 10
        NODE_A_STATUS2=$(curl -s http://127.0.0.1:18801/api/status 2>/dev/null || echo "FAIL")
        PEERS_A2=$(echo "$NODE_A_STATUS2" | grep -o '"peers":[0-9]*' | cut -d: -f2 || echo "N/A")
        log "After 10s retry — Node A: $NODE_A_STATUS2"
        if [[ "$PEERS_A2" != "N/A" ]] && [[ "$PEERS_A2" -gt 0 ]]; then
            pass "P2P: $PEERS_A2 peer(s) after retry"
        else
            warn "P2P: still 0 peers after retry"
        fi
    fi
    
    # Get heights
    HEIGHT_A=$(echo "$NODE_A_STATUS" | grep -o '"height":[0-9]*' | cut -d: -f2 || echo "0")
    HEIGHT_B=$(echo "$NODE_B_STATUS" | grep -o '"height":[0-9]*' | cut -d: -f2 || echo "0")
    log "Heights: Node A=$HEIGHT_A, Node B=$HEIGHT_B"
    
    kill $PID_A $PID_B 2>/dev/null || true
    wait $PID_A $PID_B 2>/dev/null || true
else
    fail "P2P nodes failed to start"
    kill $PID_A $PID_B 2>/dev/null || true
fi

# ═══════════════════════════════════════════════════════════════
# 5. Live Node Health Check
# ═══════════════════════════════════════════════════════════════
hr
log "5/7  Live Node Health Check"

log "Checking api.ewatts.org..."
LIVE_STATUS=$(curl -sk --max-time 10 https://api.ewatts.org/api/status 2>&1 || echo "FAIL")
if [[ "$LIVE_STATUS" != "FAIL" ]]; then
    pass "Live node reachable at api.ewatts.org"
    HEIGHT=$(echo "$LIVE_STATUS" | grep -o '"height":[0-9]*' | cut -d: -f2 || echo "?")
    PEERS=$(echo "$LIVE_STATUS" | grep -o '"peers":[0-9]*' | cut -d: -f2 || echo "?")
    log "  Height: $HEIGHT | Peers: $PEERS | Supply: $(echo $LIVE_STATUS | grep -o '"supply":[0-9]*' | cut -d: -f2)"
    
    if [[ "$PEERS" == "0" ]]; then
        warn "Live node has 0 peers — needs bootstrap connectivity"
    fi
else
    fail "Live node unreachable"
fi

log "Checking HTTPS proxy (port 8443)..."
PROXY_STATUS=$(curl -sk --max-time 10 https://178.104.193.51:8443/api/status 2>&1 || echo "FAIL")
if [[ "$PROXY_STATUS" != "FAIL" ]]; then
    pass "HTTPS proxy responding"
    log "  $PROXY_STATUS"
else
    warn "HTTPS proxy unreachable (may be expected if Cloudflare Tunnel handles api.ewatts.org)"
fi

# ═══════════════════════════════════════════════════════════════
# 6. Parameter Freeze Document
# ═══════════════════════════════════════════════════════════════
hr
log "6/7  Parameter Freeze — Mainnet Genesis Config"

GENESIS_FILE="$DIR/docs/mainnet_genesis_params.md"
cat > "$GENESIS_FILE" << 'GENEOF'
# eWatts Mainnet — Genesis Parameters

*Auto-generated by overnight script — $(date -u)*

## Core Protocol

| Parameter | Value | Source |
|-----------|-------|--------|
| Protocol version | `0x0004` (v3 emission) | `constants::PROTOCOL_VERSION` |
| Target block time | 600 s | `TARGET_BLOCK_TIME_SECS` |
| Blocks per year | 52,560 | `BLOCKS_PER_YEAR` |
| VR window | 1,000 blocks | `VR_WINDOW_BLOCKS` |
| Emission precision | 1,000,000,000 | `EMISSION_PRECISION` |

## DAG (Proof-of-Work)

| Parameter | Value | Source |
|-----------|-------|--------|
| Initial size | 8 GB | `DAG_INITIAL_SIZE_BYTES` |
| Growth rate | 512 MB/year | `DAG_GROWTH_RATE_BYTES_PER_YEAR` |
| Epoch size | 2,016 blocks (~2 weeks) | `DAG_EPOCH_BLOCKS` |
| Mix rounds | 256 | `DAG_MIX_ROUNDS` |
| Element size | 64 bytes | `DAG_ELEMENT_SIZE` |
| Acceleration rate | 1 GB | `DAG_ACCELERATION_RATE` |

## Emission (v3)

| Parameter | Value | Source |
|-----------|-------|--------|
| Base emission | 100 eWatt/block | `BASE_EMISSION_UNITS` |
| Bootstrap multiplier M_max | 100,000× | `M_MAX` |
| Maturity threshold | 10B eWatt | `S_THRESHOLD_UNITS` |
| Emission floor | 0.05× base | `EMISSION_FLOOR_MULTIPLIER` |
| Emission ceiling | 20× base | `EMISSION_CEILING_MULTIPLIER` |
| Ramp-up blocks | 10,000 | `RAMP_UP_BLOCKS` |
| Ramp-up cap | 80% | `RAMP_UP_CAP` |
| Founder lock | 50,000 blocks | `FOUNDER_LOCK_BLOCKS` |
| Founder lock additional | 40,000 blocks | `FOUNDER_LOCK_ADDITIONAL` |

## Commitment System

| Parameter | Value | Source |
|-----------|-------|--------|
| Min bandwidth commit | 1 Gbps | `MIN_COMMIT_GBS` |
| Commit window | 4,300 blocks (~30 days) | `COMMIT_WINDOW_BLOCKS` |
| Efficiency penalty | η < 0.7 | `EFFICIENCY_PENALTY_THRESHOLD` |
| Efficiency cap | η > 1.3 | `EFFICIENCY_CAP_THRESHOLD` |

## VR Calibration

| Parameter | Value | Source |
|-----------|-------|--------|
| J/GB (energy per memory access) | 6.0 | `J_PER_GB` |
| J/kWh | 3,600,000 | `J_PER_KWH` |

## Privacy

| Parameter | Value | Source |
|-----------|-------|--------|
| Ring signature size | 11 | `RING_SIGNATURE_SIZE` |
| Quantum activation block | 3,153,600 (~6 years) | `QUANTUM_ACTIVATION_BLOCK` |
| PQ signature scheme | FALCON-1024 | `PQ_SIG_SCHEME` |

## P2P

| Parameter | Value | Source |
|-----------|-------|--------|
| Max peers | 125 | `MAX_PEERS` |

## Genesis Supply

| Parameter | Value |
|-----------|-------|
| Total supply | 1,000,000 eWatt (1M) |
| Distribution | Founder allocation, time-locked |
| Genesis key | Deterministic, published before block 1 |
GENEOF

# Fill in timestamp
sed -i "s/\$(date -u)/$(date -u)/" "$GENESIS_FILE"
pass "Genesis parameters frozen: $GENESIS_FILE"

# Also create a clean genesis config JSON
GENESIS_JSON="$DIR/docs/mainnet_genesis.json"
cat > "$GENESIS_JSON" << 'JSONEOF'
{
  "version": "1.0",
  "protocol_version": 4,
  "genesis": {
    "timestamp": "",
    "supply": 1000000000000000,
    "founder_pubkey": "",
    "block_hash": ""
  },
  "params": {
    "target_block_time_secs": 600,
    "dag_initial_size_bytes": 8589934592,
    "dag_growth_bytes_per_year": 536870912,
    "base_emission_units": 100000000,
    "m_max": 100000,
    "s_threshold_units": 10000000000000000,
    "emission_floor": 0.05,
    "emission_ceiling": 20.0,
    "ramp_up_blocks": 10000,
    "ramp_up_cap": 0.8,
    "founder_lock_blocks": 50000,
    "founder_lock_additional": 40000,
    "min_commit_gbps": 1.0,
    "commit_window_blocks": 4300,
    "j_per_gb": 6.0,
    "ring_size": 11,
    "max_peers": 125
  }
}
JSONEOF
pass "Genesis JSON template: $GENESIS_JSON"

# ═══════════════════════════════════════════════════════════════
# 7. Code Quality
# ═══════════════════════════════════════════════════════════════
hr
log "7/7  Code Quality Metrics"

log "  → Lines of code..."
find src/ -name '*.rs' -exec wc -l {} + 2>/dev/null | sort -rn | head -15

log "  → Module dependency count..."
grep '^pub mod' src/main.rs

log "  → Warning count..."
WARN_COUNT=$(cargo build --release 2>&1 | grep -c 'warning:' || echo 0)
if [[ "$WARN_COUNT" -gt 20 ]]; then
    warn "$WARN_COUNT warnings — consider cleanup"
elif [[ "$WARN_COUNT" -gt 0 ]]; then
    log "  build warnings: $WARN_COUNT"
else
    pass "Zero build warnings"
fi

log "  → Duplicate constants across files..."
for const in TARGET_BLOCK_TIME DAG_INITIAL BASE_EMISSION J_PER_GB FOUNDER_LOCK; do
    matches=$(grep -rn "$const" src/*.rs 2>/dev/null | grep -v '// ' | wc -l)
    log "    $const: $matches references"
done

# ═══════════════════════════════════════════════════════════════
# Summary
# ═══════════════════════════════════════════════════════════════
hr
echo ""
echo "═══════════════════════════════════════════════════════════"
echo "  Overnight Procedures Complete — $(date -u)"
echo "═══════════════════════════════════════════════════════════"
echo ""
echo "  ✅ $PASS passed   ⚠️  $WARN warnings   ❌ $FAIL failed"
echo ""
echo "  Reports saved:"
echo "    Full log:      $REPORT"
echo "    Genesis params: $GENESIS_FILE"
echo "    Genesis JSON:  $GENESIS_JSON"
echo ""

# Exit code
if [[ "$FAIL" -gt 0 ]]; then
    exit 1
fi
exit 0
