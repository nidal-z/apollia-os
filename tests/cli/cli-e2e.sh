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
# The seed is the shared, committed fixture at tests/cli/seed (one
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
# Exit code: 0 when every assertion passed, 1 when one failed, 2 when the suite
# refused to start and measured nothing (no binary to run).

set -uo pipefail

# ── Locate repo + libs ─────────────────────────────────────────────────────
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." >/dev/null 2>&1 && pwd)"
LIB_DIR="$SCRIPT_DIR/lib"
TRACK_DIR="$SCRIPT_DIR/tracks"

# ── Resolve binary ─────────────────────────────────────────────────────────
if [[ -n "${APOLLIA_BIN:-}" ]]; then
    # Resolved against the directory the suite was invoked from, once, here.
    # One assertion runs the binary after a `cd` (workspace init --force,
    # tracks/track1_offline.sh), and a relative path stops resolving there. The
    # suite then reported 153 PASS and a single FAIL naming that assertion,
    # which is a red pointing at the wrong thing: the input was malformed, not
    # the command under test.
    BIN=$APOLLIA_BIN
    [[ "$BIN" == /* ]] || BIN="$PWD/$BIN"
    if [[ ! -x "$BIN" ]]; then
        echo "REFUSING: APOLLIA_BIN resolves to $BIN, which is not executable." >&2
        echo "          No assertion was run. Build with \`cargo build -p apollia-cli\`, or point APOLLIA_BIN at a binary." >&2
        exit 2
    fi
elif [[ -x "$REPO_ROOT/target/release/apollia-os" ]]; then
    BIN="$REPO_ROOT/target/release/apollia-os"
elif [[ -x "$REPO_ROOT/target/debug/apollia-os" ]]; then
    BIN="$REPO_ROOT/target/debug/apollia-os"
else
    echo "REFUSING: apollia-os binary not found. Build with \`cargo build -p apollia-cli\` or set APOLLIA_BIN." >&2
    echo "          No assertion was run." >&2
    exit 2
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
# Hermetic config resolution. `config show` resolves $XDG_CONFIG_HOME before
# ~/.config (resolve_path, crates/apollia-cli/src/commands/config.rs), and the
# suite only swaps HOME, so a host-set XDG_CONFIG_HOME points every config
# assertion at a directory outside the seed and flips its verdict. With the
# variable dropped, resolution falls back to $HOME/.config, which follows the
# per-track HOME swaps into the seeded tree (build-seed.sh writes the seeded
# apollia.toml there as well as under ~/.apollia).
unset XDG_CONFIG_HOME
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
echo "  SEED builder      = $REPO_ROOT/tests/cli/seed/build-seed.sh"
echo "  TEST_GGUF         = $TEST_GGUF ($([[ -f "$TEST_GGUF" ]] && echo present || echo 'absent → Track 3 SKIP'))"
echo "  REQUIRE_RUNTIME   = $REQUIRE_RUNTIME"
echo "  REPORT_DIR        = $REPORT_DIR"

# ═══════════════════════════════════════════════════════════════════════════
#                            TRACK 1 - OFFLINE
# ═══════════════════════════════════════════════════════════════════════════
echo
echo "$(bold "═══ Track 1 - OFFLINE (seeded HOME, no daemon) ═══")"
SEED1="$RUN_TMP/seed-offline"
# The builder's diagnostic is captured rather than let through, so the recorded
# detail carries the reason instead of nothing, and echoed back so an operator
# watching the run still reads it.
if seed_err=$(build_seed_home "$SEED1" 2>&1); then
    export HOME="$SEED1"
    CURRENT_TRACK="offline"
    # shellcheck source=tracks/track1_offline.sh
    source "$TRACK_DIR/track1_offline.sh"
    export HOME="$REAL_HOME"
else
    echo "$(red "FAIL"): could not build the seeded HOME for Track 1." >&2
    printf '%s\n' "$seed_err" >&2
    _record_fail "seed build (Track 1)" "$seed_err"
fi

# ═══════════════════════════════════════════════════════════════════════════
#                        TRACK 2 + 3 - RUNTIME
# ═══════════════════════════════════════════════════════════════════════════
if [[ "$REQUIRE_RUNTIME" == "1" ]]; then
    echo
    echo "$(bold "═══ Track 2 - RUNTIME (daemon on seeded HOME) ═══")"
    SEED2="$RUN_TMP/seed-runtime"
    if seed_err=$(build_seed_home "$SEED2" 2>&1); then
        export HOME="$SEED2"
        MODEL_READY=0
        if [[ -f "$TEST_GGUF" ]]; then
            # Repoint the seeded default backend at the real model by ABSOLUTE
            # path (copy-free, read-only). Never symlink the real models dir or
            # copy the model: `llm setup --models-dir <real dir>` would truncate
            # the source model to 0 bytes (copy-onto-itself hazard).
            seed_wire_real_model "$SEED2" "$TEST_GGUF" && MODEL_READY=1
        fi
        # `--port 0` leaves the port choice to the process that will hold it:
        # the kernel assigns one to the daemon's own listener, so there is no
        # window in which a third party can take it. Picking a port here and
        # passing the number would reopen that window, and the window spanned
        # the whole offline track, since the number was read before the banner
        # and bound only after Track 1 finished. Any process consuming
        # ephemeral ports meanwhile won it, and the suite then failed on
        # "daemon start" for a reason foreign to the product: `failed to bind
        # TCP on port <n>: Address already in use`, or, when the winner was
        # listening, the false `runtime already running on localhost:<n>`.
        # Nothing in the suite talks to the TCP port; every assertion goes
        # through --socket, so the number never has to be known.
        "$BIN" start --socket "$SOCK" --port 0 >"$RUN_TMP/daemon.log" 2>&1 &
        DAEMON_PID=$!
        if wait_for_socket "$SOCK" 30; then
            # The port is deliberately not reported: the suite never learns the
            # number, and printing the requested 0 where a reader looks for the
            # bound port would state a value nothing measured.
            _pass "daemon started + socket ready" "pid=$DAEMON_PID port=assigned by the kernel"
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
            # The detail carries the log, not its path: $RUN_TMP is removed by
            # the EXIT trap before anyone reads the report.
            daemon_tail=$(/usr/bin/tail -30 "$RUN_TMP/daemon.log" 2>/dev/null)
            # CURRENT_TRACK is still `offline` here: the runtime assignment sits
            # further down, past the branch that only a successful boot reaches.
            CURRENT_TRACK="runtime"
            _record_fail "daemon start" "daemon.log (tail): $daemon_tail"
            echo "--- daemon.log (tail) ---"; printf '%s\n' "$daemon_tail"; echo "--- end ---"
        fi
        export HOME="$REAL_HOME"
    else
        printf '%s\n' "$seed_err" >&2
        CURRENT_TRACK="runtime"
        _record_fail "seed build (Track 2)" "$seed_err"
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
# Command-coverage: enumerate every clap leaf and classify it against the REAL
# invocations of the track sources, appended to report.md. The floor (zero
# leaves without a track) is enforced by scripts/check_cli_e2e_coverage.py in
# `just guards` and in CI; here a violation is surfaced without flipping the
# suite's own verdict, which is about the assertions that ran.
if ! /usr/bin/python3 "$LIB_DIR/coverage.py" --bin "$BIN" --tracks-dir "$TRACK_DIR" \
    --append-md "$REPORT_DIR/report.md"; then
    echo "$(yellow "WARNING"): command-coverage floor violated or unmeasured;" \
         "run python3 scripts/check_cli_e2e_coverage.py for the verdict." >&2
fi

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
