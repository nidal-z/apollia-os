#!/usr/bin/env bash
# tests/cli/cli-e2e.sh - Apollia OS CLI end-to-end suite (orchestrator).
#
# Runs the CLI against a FIXED, isolated, deterministically-seeded HOME so that
# read commands assert KNOWN content (not empty states), and produces a
# structured report (report.json + report.md). Three tracks:
#
#   Track 1 - OFFLINE (always): every command runnable without the daemon,
#     against a fresh seeded HOME. Content assertions + exit-code contract.
#   Track 2 - RUNTIME (gated APOLLIA_REQUIRE_RUNTIME=1): daemon booted on a
#     seeded HOME, so status/list surfaces return seeded state; plus CRUD and
#     the runtime-only leaves.
#   Track 3 - LLM CAPTURE (gated APOLLIA_REQUIRE_RUNTIME=1 + a real model):
#     non-deterministic commands (run --stream, chat REPL, llm chat, do,
#     explain). Asserts STRUCTURE only (exit, streaming, timing); the full
#     input/output is captured into report.md for human review.
#
# The seed is the shared, committed fixture at scripts/automation/seed (one
# source of truth with the desktop automation suite). HOME is swapped per phase;
# the real ~/.apollia is never touched.
#
# Environment variables:
#   APOLLIA_BIN              binary path. Default: ./target/release/apollia-os
#                            (fallback ./target/debug/apollia-os).
#   APOLLIA_TEST_MODEL_GGUF  GGUF for the local LLM backend (Track 3). Default:
#                            ~/.apollia/models/Qwen3-30B-A3B-Q4_K_M.gguf. Absent
#                            → Track 3 is SKIPped, never failed.
#   APOLLIA_REQUIRE_RUNTIME  0|1 (default 0). Set to 1 to run Tracks 2 and 3.
#   APOLLIA_TEST_REVIEW      0|1 (default 0). Set to 1 to run `review .` (slow).
#   APOLLIA_TEST_VERBOSE     0|1 (default 0). Dump stdout/stderr on FAIL.
#   APOLLIA_E2E_REPORT_DIR   report output dir. Default: tests/cli/report.
#
# Exit code: 0 when every assertion passed, 1 otherwise.

set -uo pipefail

# ── Locate repo + libs ─────────────────────────────────────────────────────
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." >/dev/null 2>&1 && pwd)"
LIB_DIR="$SCRIPT_DIR/lib"
TRACK_DIR="$SCRIPT_DIR/tracks"

# ── Resolve binary ─────────────────────────────────────────────────────────
if [[ -n "${APOLLIA_BIN:-}" ]]; then
    BIN=$APOLLIA_BIN
elif [[ -x "$REPO_ROOT/target/release/apollia-os" ]]; then
    BIN="$REPO_ROOT/target/release/apollia-os"
elif [[ -x "$REPO_ROOT/target/debug/apollia-os" ]]; then
    BIN="$REPO_ROOT/target/debug/apollia-os"
else
    echo "FAIL: apollia-os binary not found. Build with \`cargo build -p apollia-cli\` or set APOLLIA_BIN." >&2
    exit 1
fi

REAL_HOME=$HOME
DEFAULT_GGUF="${REAL_HOME}/.apollia/models/Qwen3-30B-A3B-Q4_K_M.gguf"
TEST_GGUF="${APOLLIA_TEST_MODEL_GGUF:-$DEFAULT_GGUF}"
REQUIRE_RUNTIME="${APOLLIA_REQUIRE_RUNTIME:-0}"
TEST_REVIEW="${APOLLIA_TEST_REVIEW:-0}"
VERBOSE="${APOLLIA_TEST_VERBOSE:-0}"
REPORT_DIR="${APOLLIA_E2E_REPORT_DIR:-$SCRIPT_DIR/report}"

# Hermetic secret storage (keyring is unreachable from a sub-shell on macOS).
export APOLLIA_TOKEN_STORAGE="${APOLLIA_TOKEN_STORAGE:-file}"
export APOLLIA_TOKEN_PASSPHRASE="${APOLLIA_TOKEN_PASSPHRASE:-cli-e2e-test-passphrase}"
# Keep INFO tracing off stdout so JSON-emitting commands stay parseable.
export RUST_LOG="${RUST_LOG:-error}"

# ── Load libraries ─────────────────────────────────────────────────────────
# shellcheck source=lib/assert.sh
source "$LIB_DIR/assert.sh"
# shellcheck source=lib/report.sh
source "$LIB_DIR/report.sh"
# shellcheck source=lib/seed.sh
source "$LIB_DIR/seed.sh"

# ── Run workspace + cleanup ────────────────────────────────────────────────
RUN_TMP=$(/usr/bin/mktemp -d -t apollia-cli-e2e.XXXXXX)
DAEMON_PID=""
SOCK="$RUN_TMP/apollia.sock"
FREE_PORT=$(/usr/bin/python3 -c "import socket;s=socket.socket();s.bind(('127.0.0.1',0));print(s.getsockname()[1]);s.close()" 2>/dev/null || echo "0")

cleanup() {
    local rc=${1:-0}
    if [[ -n "$DAEMON_PID" ]] && /bin/kill -0 "$DAEMON_PID" 2>/dev/null; then
        "$BIN" --socket "$SOCK" stop >/dev/null 2>&1 || /bin/kill -TERM "$DAEMON_PID" 2>/dev/null
        /usr/bin/wait "$DAEMON_PID" 2>/dev/null || true
    fi
    /bin/rm -rf "$RUN_TMP"
    exit "$rc"
}
trap 'cleanup $?' EXIT
trap 'cleanup 130' INT TERM

wait_for_socket() {
    local sock=$1 timeout=${2:-30} i=0
    while [[ $i -lt $timeout ]]; do
        "$BIN" --socket "$sock" status >/dev/null 2>&1 && return 0
        /bin/sleep 1; i=$((i + 1))
    done
    return 1
}

# ── Counters + report ──────────────────────────────────────────────────────
PASS=0; FAIL=0; SKIP=0; FAILED_LABELS=()
CURRENT_TRACK="offline"
report_init "$RUN_TMP"
# Wall clock via `date +%s`: portable across bash 3.2 (no EPOCHREALTIME) and 5.x.
WALL0=$(/bin/date +%s)

# ── Banner ─────────────────────────────────────────────────────────────────
echo "$(bold "Apollia OS CLI E2E suite")"
echo "  BIN               = $BIN"
echo "  SEED builder      = $REPO_ROOT/scripts/automation/seed/build-seed.sh"
echo "  TEST_GGUF         = $TEST_GGUF ($([[ -f "$TEST_GGUF" ]] && echo present || echo 'absent → Track 3 SKIP'))"
echo "  REQUIRE_RUNTIME   = $REQUIRE_RUNTIME"
echo "  REPORT_DIR        = $REPORT_DIR"

# ═══════════════════════════════════════════════════════════════════════════
#                            TRACK 1 - OFFLINE
# ═══════════════════════════════════════════════════════════════════════════
echo
echo "$(bold "═══ Track 1 - OFFLINE (seeded HOME, no daemon) ═══")"
SEED1="$RUN_TMP/seed-offline"
if build_seed_home "$SEED1"; then
    export HOME="$SEED1"
    CURRENT_TRACK="offline"
    # shellcheck source=tracks/track1_offline.sh
    source "$TRACK_DIR/track1_offline.sh"
    export HOME="$REAL_HOME"
else
    echo "$(red "FAIL"): could not build the seeded HOME for Track 1." >&2
    _record_fail "seed build (Track 1)" ""
fi

# ═══════════════════════════════════════════════════════════════════════════
#                        TRACK 2 + 3 - RUNTIME
# ═══════════════════════════════════════════════════════════════════════════
if [[ "$REQUIRE_RUNTIME" == "1" ]]; then
    echo
    echo "$(bold "═══ Track 2 - RUNTIME (daemon on seeded HOME) ═══")"
    SEED2="$RUN_TMP/seed-runtime"
    if build_seed_home "$SEED2"; then
        export HOME="$SEED2"
        MODEL_READY=0
        if [[ -f "$TEST_GGUF" ]]; then
            # Repoint the seeded default backend at the real model by ABSOLUTE
            # path (copy-free, read-only). Never symlink the real models dir or
            # copy the model: `llm setup --models-dir <real dir>` would truncate
            # the source model to 0 bytes (copy-onto-itself hazard).
            seed_wire_real_model "$SEED2" "$TEST_GGUF" && MODEL_READY=1
        fi
        "$BIN" start --socket "$SOCK" --port "$FREE_PORT" >"$RUN_TMP/daemon.log" 2>&1 &
        DAEMON_PID=$!
        if wait_for_socket "$SOCK" 30; then
            _pass "daemon started + socket ready" "pid=$DAEMON_PID port=$FREE_PORT"
            # When a real model is boot-loaded, wait for it to answer before any
            # track runs, so LLM-metadata commands never race the model load.
            if [[ "$MODEL_READY" == "1" ]]; then
                if seed_wait_model_ready 40; then
                    _pass "local model loaded + ready"
                else
                    echo "  $(yellow ⊘) local model did not become ready in 80s; Track 3 will skip"
                    MODEL_READY=0
                fi
            fi
            CURRENT_TRACK="runtime"
            # shellcheck source=tracks/track2_runtime.sh
            source "$TRACK_DIR/track2_runtime.sh"

            echo
            echo "$(bold "═══ Track 3 - LLM CAPTURE (non-deterministic) ═══")"
            CURRENT_TRACK="llm"
            if [[ "$MODEL_READY" == "1" ]]; then
                # shellcheck source=tracks/track3_llm.sh
                source "$TRACK_DIR/track3_llm.sh"
            else
                skip "Track 3 (LLM capture)" "no real model wired (APOLLIA_TEST_MODEL_GGUF absent)"
            fi

            "$BIN" --socket "$SOCK" stop >/dev/null 2>&1 || true
            DAEMON_PID=""
        else
            _record_fail "daemon start" "see $RUN_TMP/daemon.log"
            echo "--- daemon.log (tail) ---"; /usr/bin/tail -30 "$RUN_TMP/daemon.log" 2>/dev/null; echo "--- end ---"
        fi
        export HOME="$REAL_HOME"
    else
        _record_fail "seed build (Track 2)" ""
    fi
else
    echo
    echo "$(yellow "Tracks 2 + 3 SKIPPED") (set APOLLIA_REQUIRE_RUNTIME=1 to enable)"
fi

# ═══════════════════════════════════════════════════════════════════════════
#                                 SUMMARY
# ═══════════════════════════════════════════════════════════════════════════
WALL=$(( $(/bin/date +%s) - WALL0 ))
/bin/mkdir -p "$REPORT_DIR"
report_finalize "$REPORT_DIR/report.json" "$REPORT_DIR/report.md" "$PASS" "$FAIL" "$SKIP" "$WALL"
# Command-coverage: enumerate every clap leaf and classify exercised / skipped /
# uncovered against the track sources, appended to report.md.
/usr/bin/python3 "$LIB_DIR/coverage.py" --bin "$BIN" --tracks-dir "$TRACK_DIR" \
    --append-md "$REPORT_DIR/report.md" 2>/dev/null || true

echo
echo "$(bold "═══ Summary ═══")"
printf '  PASS  : %d\n' "$PASS"
printf '  FAIL  : %d\n' "$FAIL"
printf '  SKIP  : %d\n' "$SKIP"
printf '  WALL  : %ds\n' "$WALL"
printf '  REPORT: %s\n' "$REPORT_DIR/report.md"

if [[ "$FAIL" -gt 0 ]]; then
    echo
    echo "$(red "Failed assertions:")"
    for label in "${FAILED_LABELS[@]}"; do echo "  - $label"; done
    exit 1
fi
echo "$(green "All assertions passed.")"
exit 0
