#!/usr/bin/env bash
# eWatts Steady-State Energy Benchmark
# Roda o benchmark de mineração contínua com opções flexíveis.
# Uso: ./scripts/energy_benchmark.sh [opções]

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$(dirname "$SCRIPT_DIR")"
cd "$REPO_DIR"

# Defaults
DAG_SIZE_MB=256
DIFFICULTY=1000
DURATION_SECS=120
THREADS=1
REPORT_INTERVAL=10
RELEASE=true
TRACKING_DIR="tracking"
TIMESTAMP=$(date -u +"%Y%m%d_%H%M%S")
OUTFILE="${TRACKING_DIR}/bench_${TIMESTAMP}.txt"

# Parse args
while [[ $# -gt 0 ]]; do
    case "$1" in
        --dag-size) DAG_SIZE_MB="$2"; shift 2 ;;
        --difficulty) DIFFICULTY="$2"; shift 2 ;;
        --duration) DURATION_SECS="$2"; shift 2 ;;
        --threads) THREADS="$2"; shift 2 ;;
        --report-interval) REPORT_INTERVAL="$2"; shift 2 ;;
        --debug) RELEASE=false; shift ;;
        --help)
            echo "Uso: $0 [opções]"
            echo "  --dag-size <MB>     DAG size em MB (default: 256)"
            echo "  --difficulty <n>    Dificuldade de mining (default: 1000)"
            echo "  --duration <s>      Duração em segundos (default: 120)"
            echo "  --threads <n>       Threads paralelas (default: 1)"
            echo "  --report-interval <s> Intervalo de relatório (default: 10)"
            echo "  --debug             Build debug (default: release)"
            exit 0
            ;;
        *) echo "Unknown: $1"; exit 1 ;;
    esac
    shift
done

mkdir -p "$TRACKING_DIR"

echo "======================================"
echo " eWatts Energy Benchmark"
echo "======================================"
echo " DAG size:      ${DAG_SIZE_MB} MB"
echo " Difficulty:    ${DIFFICULTY}"
echo " Duration:      ${DURATION_SECS}s"
echo " Threads:       ${THREADS}"
echo " Report every:  ${REPORT_INTERVAL}s"
echo " Output:        ${OUTFILE}"
echo "======================================"

# Build
BUILD_FLAGS="--features testnet"
if $RELEASE; then
    BUILD_FLAGS="$BUILD_FLAGS --release"
fi
echo "Building steady-bench..."
cargo build --bin steady-bench $BUILD_FLAGS 2>&1 | tail -2

# Run benchmark
BIN="./target/$([ $RELEASE = true ] && echo 'release' || echo 'debug')/steady-bench"

echo ""
echo "Starting benchmark..."
echo ""

$BIN \
    --dag-size "$DAG_SIZE_MB" \
    --difficulty "$DIFFICULTY" \
    --duration "$DURATION_SECS" \
    --threads "$THREADS" \
    --report-interval "$REPORT_INTERVAL" \
    2>&1 | tee "$OUTFILE"

echo ""
echo "Benchmark complete. Results saved to: $OUTFILE"
