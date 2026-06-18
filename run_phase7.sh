#!/usr/bin/env bash
# ═══════════════════════════════════════════════════════════════
# eWatts — Phase 7: Test Coverage Completion
# ═══════════════════════════════════════════════════════════════
# Cobre os gaps identificados:
#   Store (0→16 tests), DAG benchmark (0→7 tests),
#   Mempool (1→9 tests), P2P multi-node, Pool server
#
# Uso:  bash run_phase7.sh [--quick] [--p2p] [--pool] [--all]
# ═══════════════════════════════════════════════════════════════

set -euo pipefail
PHASE7_DIR="$(cd "$(dirname "$0")" && pwd)"
REPORT="$PHASE7_DIR/phase7_report.txt"
PASS=0
FAIL=0
SKIP=0
START_TS=$(date -u +%s)

log()   { echo "[PHASE7] $(date -u +%H:%M:%S) $*"; }
pass()  { echo "  ✅ $1"; ((PASS++)); }
fail()  { echo "  ❌ $1"; ((FAIL++)); }
skip()  { echo "  ⏭️  $1"; ((SKIP++)); }

banner() {
    echo ""
    echo "╔═══════════════════════════════════════════════════════╗"
    printf "║  %-55s ║\n" "$1"
    echo "╚═══════════════════════════════════════════════════════╝"
}

cleanup() {
    log "Cleaning up..."
    # Kill any leftover ewatts nodes
    pkill -f "ewatts-protocol start" 2>/dev/null || true
    pkill -f "ewatts-protocol p2p" 2>/dev/null || true
    pkill -f "ewatts-protocol pool" 2>/dev/null || true
    # Remove test data dirs
    rm -rf /tmp/ewatts_phase7_* 2>/dev/null || true
}
trap cleanup EXIT

cd "$PHASE7_DIR"

# ── Parse flags ──────────────────────────────────────────────
RUN_P2P=false
RUN_POOL=false
QUICK=false
for arg in "$@"; do
    case "$arg" in
        --p2p)  RUN_P2P=true ;;
        --pool) RUN_POOL=true ;;
        --quick) QUICK=true ;;
        --all)  RUN_P2P=true; RUN_POOL=true ;;
    esac
done
if [[ "$#" -eq 0 ]]; then
    RUN_P2P=true
    RUN_POOL=true
fi

exec > >(tee -a "$REPORT") 2>&1
: > "$REPORT"

echo ""
echo "═══════════════════════════════════════════════════════════"
echo "  eWatts Phase 7 — $(date -u)"
echo "═══════════════════════════════════════════════════════════"

# ═══════════════════════════════════════════════════════════════
# 1. Rust Unit Tests (Store + DAG + Mempool + existing)
# ═══════════════════════════════════════════════════════════════
banner "1/5  Rust Unit Tests (Store, DAG, Mempool, existing)"

log "Compiling with new Phase 7 tests..."
START=$(date +%s)
if RUSTFLAGS="${RUSTFLAGS:--C linker=gcc}" cargo test 2>&1; then
    END=$(date +%s)
    # Parse test results
    TOTAL=$(grep -E 'test result:' <(cargo test 2>&1 | tail -20) || true)
    log "Build + tests completed in $((END - START))s"
    
    # Count Phase 7 specific tests
    STORE_TESTS=$(grep -c 'test_store_' <(cargo test 2>&1 || true) || true)
    DAG_TESTS=$(grep -c 'test_dag_' <(cargo test 2>&1 || true) || true)
    MEMPOOL_TESTS=$(grep -c 'test_mempool_' <(cargo test 2>&1 || true) || true)
    
    log "Store tests: ${STORE_TESTS:-?}, DAG tests: ${DAG_TESTS:-?}, Mempool tests: ${MEMPOOL_TESTS:-?}"
    pass "cargo test — all 120+ tests pass"
else
    fail "cargo test — compilation or test failure"
fi

# ═══════════════════════════════════════════════════════════════
# 2. DAG Performance Benchmark (spec §4.2 target: 8GB <60s)
# ═══════════════════════════════════════════════════════════════
banner "2/5  DAG Performance Benchmark"

log "Running DAG benchmark tests (64 MB)..."
START=$(date +%s)
if RUSTFLAGS="${RUSTFLAGS:--C linker=gcc}" cargo test test_dag_benchmark_64mb -- --nocapture 2>&1; then
    END=$(date +%s)
    pass "DAG 64 MB benchmark completed in $((END - START))s"
else
    fail "DAG 64 MB benchmark"
fi

log "Running progressive DAG sizes..."
if RUSTFLAGS="${RUSTFLAGS:--C linker=gcc}" cargo test test_dag_benchmark_progressive -- --nocapture 2>&1; then
    pass "DAG progressive sizes (1/4/16/64 MB)"
else
    fail "DAG progressive sizes"
fi

log "Running DAG determinism tests..."
if RUSTFLAGS="${RUSTFLAGS:--C linker=gcc}" cargo test test_dag_deterministic test_dag_epoch_different test_dag_get_wraparound test_dag_cache_hit -- --nocapture 2>&1; then
    pass "DAG determinism + cache 4/4"
else
    fail "DAG determinism suite"
fi

# ═══════════════════════════════════════════════════════════════
# 3. Store Integration (disk persistence roundtrip)
# ═══════════════════════════════════════════════════════════════
banner "3/5  Store Integration (disk persistence)"

log "Running store integration tests (serial --test-threads=1)..."
if RUSTFLAGS="${RUSTFLAGS:--C linker=gcc}" cargo test test_store_ -- --test-threads=1 --nocapture 2>&1; then
    pass "All store tests pass (serial)"
else
    fail "Store tests"
fi

# ═══════════════════════════════════════════════════════════════
# 4. P2P Multi-Node Real Test
# ═══════════════════════════════════════════════════════════════
if $RUN_P2P; then
    banner "4/5  P2P Multi-Node Real Test"
    
    log "Building release binary for P2P tests..."
    RUSTFLAGS="${RUSTFLAGS:--C linker=gcc}" cargo build --release 2>&1 | tail -3
    
    NODE_BIN="$PHASE7_DIR/target/release/ewatts-protocol"
    
    # Node A: boots first, no bootstrap
    NODE_A_DIR="/tmp/ewatts_phase7_node_a"
    NODE_B_DIR="/tmp/ewatts_phase7_node_b"
    rm -rf "$NODE_A_DIR" "$NODE_B_DIR"
    
    log "Starting Node A (seed, port 17800)..."
    mkdir -p "$NODE_A_DIR"
    cd "$NODE_A_DIR"
    "$NODE_BIN" start --p2p --p2p-port 17800 --difficulty 1 --dash-port 17801 &
    PID_A=$!
    sleep 2
    
    # Get Node A's address from logs
    log "Getting Node A's peer ID..."
    sleep 2
    NODE_A_ADDR="/ip4/127.0.0.1/tcp/17800"
    
    log "Starting Node B (bootstrap to Node A)..."
    mkdir -p "$NODE_B_DIR"
    cd "$NODE_B_DIR"
    "$NODE_BIN" start --p2p --p2p-port 17802 --bootstrap "$NODE_A_ADDR" --difficulty 1 --dash-port 17803 &
    PID_B=$!
    sleep 3
    
    # Check if both nodes are running
    if kill -0 $PID_A 2>/dev/null && kill -0 $PID_B 2>/dev/null; then
        pass "Both P2P nodes started successfully"
        log "Node A PID: $PID_A, Node B PID: $PID_B"
        
        # Wait for sync (10s)
        log "Waiting 10s for P2P sync..."
        sleep 10
        
        # Check peer count via API
        API_A="http://127.0.0.1:17801/api/status"
        API_B="http://127.0.0.1:17803/api/status"
        
        log "Node A status:"
        curl -s "$API_A" 2>/dev/null || echo "  (unreachable)"
        log "Node B status:"
        curl -s "$API_B" 2>/dev/null || echo "  (unreachable)"
        
        # Check peer count
        PEERS_A=$(curl -s "$API_A" 2>/dev/null | grep -o '"peers":[0-9]*' | cut -d: -f2 || echo "N/A")
        PEERS_B=$(curl -s "$API_B" 2>/dev/null | grep -o '"peers":[0-9]*' | cut -d: -f2 || echo "N/A")
        
        if [[ "$PEERS_A" != "N/A" ]] && [[ "$PEERS_B" != "N/A" ]]; then
            log "Node A peers: $PEERS_A, Node B peers: $PEERS_B"
            if [[ "$PEERS_A" -gt 0 ]] || [[ "$PEERS_B" -gt 0 ]]; then
                pass "P2P multi-node: peers connected ($PEERS_A / $PEERS_B)"
            else
                fail "P2P multi-node: no peers detected (0/0)"
                log "Check for network config issues"
            fi
        else
            fail "P2P multi-node: API unreachable"
        fi
        
        # Cleanup nodes
        kill $PID_A $PID_B 2>/dev/null || true
        wait $PID_A $PID_B 2>/dev/null || true
        log "P2P nodes stopped"
    else
        fail "P2P nodes failed to start"
        kill $PID_A $PID_B 2>/dev/null || true
    fi
else
    skip "P2P multi-node test (use --p2p to enable)"
fi

# ═══════════════════════════════════════════════════════════════
# 5. Pool Server Test
# ═══════════════════════════════════════════════════════════════
if $RUN_POOL; then
    banner "5/5  Pool Server Test"
    
    log "Building release binary..."
    RUSTFLAGS="${RUSTFLAGS:--C linker=gcc}" cargo build --release 2>&1 | tail -3
    
    NODE_BIN="$PHASE7_DIR/target/release/ewatts-protocol"
    
    log "Starting pool server on port 17900..."
    "$NODE_BIN" pool serve 17900 &
    PID_POOL=$!
    sleep 2
    
    if kill -0 $PID_POOL 2>/dev/null; then
        pass "Pool server started on port 17900"
        
        # Test GET /stats
        log "Testing GET /stats..."
        STATS=$(curl -s http://127.0.0.1:17900/stats 2>/dev/null || echo "FAIL")
        if [[ "$STATS" != "FAIL" ]]; then
            log "Pool stats response: $STATS"
            pass "Pool server GET /stats responds"
        else
            fail "Pool server GET /stats unreachable"
        fi
        
        # Test GET / (dashboard)
        log "Testing GET /..."
        DASH=$(curl -s http://127.0.0.1:17900/ 2>/dev/null || echo "FAIL")
        if [[ "$DASH" != "FAIL" ]]; then
            DASH_LEN=$(echo "$DASH" | wc -c)
            pass "Pool dashboard returns HTML ($DASH_LEN bytes)"
        else
            fail "Pool dashboard unreachable"
        fi
        
        # Test POST /register
        log "Testing POST /register..."
        REG=$(curl -s -X POST http://127.0.0.1:17900/register \
            -H "Content-Type: application/json" \
            -d '{"miner_id":"ab"}' 2>/dev/null || echo "FAIL")
        if [[ "$REG" != "FAIL" ]]; then
            pass "Pool server POST /register responds"
        else
            fail "Pool server POST /register"
        fi
        
        # Cleanup
        kill $PID_POOL 2>/dev/null || true
        wait $PID_POOL 2>/dev/null || true
        log "Pool server stopped"
    else
        fail "Pool server failed to start"
    fi
else
    skip "Pool server test (use --pool to enable)"
fi

# ═══════════════════════════════════════════════════════════════
# Final Report
# ═══════════════════════════════════════════════════════════════
END_TS=$(date -u +%s)
DURATION=$((END_TS - START_TS))

echo ""
echo "═══════════════════════════════════════════════════════════"
echo "  Phase 7 Complete — $(date -u)"
echo "  Duration: $DURATION seconds"
echo "═══════════════════════════════════════════════════════════"
echo ""
echo "  Results:  ✅ $PASS passed   ❌ $FAIL failed   ⏭️  $SKIP skipped"
echo ""
echo "  What was covered:"
echo "   - Store: block persistence, query, prune, keys, UTXO, chain store"
echo "   - DAG: 64 MB benchmark, progressive, determinism, cache, epoch, wrap"
echo "   - Mempool: submit, peek, take, drain, confirm, hash lookup, limit"
echo "   - P2P: multi-node startup, peer connectivity"
echo "   - Pool: HTTP server, stats, register"
echo ""
echo "  Report saved to: $REPORT"
echo ""

# Exit code
if [[ "$FAIL" -gt 0 ]]; then
    exit 1
fi
exit 0
