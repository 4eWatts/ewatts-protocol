#!/bin/bash
# eWatts Common — shared helpers for test scripts
# Prevents concurrent test runs from killing each other.
# Source this file at the top of any test script.
# Usage: source "$(dirname "$0")/ewatts_common.sh"

# ── Lockfile ─────────────────────────────────────────────────────────
# Only one eWatts test script runs at a time.
# Lockfile is named after the calling script to avoid confusion.
LOCKFILES_DIR="/tmp/ewatts-locks"
LOCKFILE="$LOCKFILES_DIR/$(basename "$0" .sh).lock"

acquire_lock() {
  mkdir -p "$LOCKFILES_DIR"
  if ! mkdir "$LOCKFILE" 2>/dev/null; then
    local holder
    holder=$(cat "$LOCKFILE/pid" 2>/dev/null || echo "unknown")
    echo "ERROR: Another eWatts test is already running (held by $holder)."
    echo "       Lockfile: $LOCKFILE"
    echo "       Wait for it to finish, or run: rm -rf $LOCKFILE"
    exit 1
  fi
  echo "$$" > "$LOCKFILE/pid"
  # Write timestamp for debugging
  date -u '+%Y-%m-%d %H:%M:%S UTC' > "$LOCKFILE/started"
  echo "Lock acquired: $LOCKFILE"
}

release_lock() {
  if [ -d "$LOCKFILE" ]; then
    rm -rf "$LOCKFILE"
    echo "Lock released: $LOCKFILE"
  fi
}

# ── PID-tracked kill ────────────────────────────────────────────────
# Only kills processes that this script started (tracked via PID list).
# Source scripts should add PIDs via: track_pid $PID
PID_FILE="/tmp/ewatts-pids-$$.txt"

track_pid() {
  echo "$1" >> "$PID_FILE"
}

track_pgid() {
  # Track all processes in a process group
  local pgid="$1"
  ps -o pid= --pgid "$pgid" 2>/dev/null | tr -d ' ' >> "$PID_FILE"
}

kill_tracked() {
  if [ -f "$PID_FILE" ]; then
    local pids
    pids=$(sort -u "$PID_FILE" | tr '\n' ' ')
    if [ -n "$pids" ]; then
      # Kill gently first, then force
      kill $pids 2>/dev/null || true
      sleep 1
      # Check survivors
      local survivors
      survivors=""
      for pid in $pids; do
        if kill -0 "$pid" 2>/dev/null; then
          survivors="$survivors $pid"
        fi
      done
      if [ -n "$survivors" ]; then
        kill -9 $survivors 2>/dev/null || true
      fi
    fi
    rm -f "$PID_FILE"
  fi
}

# Kill all eWatts processes (legacy, for manual cleanup).
# Not used by tracked scripts — only for emergency use.
ewatts_kill_all() {
  ps aux | grep -E "target/release/ewatts-protocol" | grep -v grep | \
    awk '{print $2}' | xargs -r kill "$@" 2>/dev/null || true
  sleep 1
}

# ── Signal handler ──────────────────────────────────────────────────
_ewatts_cleanup() {
  kill_tracked
  release_lock
}

# Register cleanup on exit, SIGINT, SIGTERM
trap _ewatts_cleanup EXIT
trap 'exit 1' INT TERM

# Auto-acquire lock on source (unless SKIP_LOCK=1)
if [ "${SKIP_LOCK:-0}" != "1" ]; then
  acquire_lock
fi

echo "eWatts common loaded — PID tracking active (file: $PID_FILE)"
