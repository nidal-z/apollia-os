# shellcheck shell=bash
# tests/cli/tracks/track3_llm.sh - non-deterministic LLM CAPTURE track.
#
# Runs the model-backed commands and asserts STRUCTURE ONLY: exit code,
# streaming happened (>= N stdout bursts), terminated within the timeout. The
# actual generated content is NEVER asserted, it is captured verbatim into the
# report (report.md "Non-deterministic captures" section) for human review, the
# way the user asked: for `chat` we prove the answer streams, not what it says.
#
# Sourced by cli-e2e.sh with the daemon up on $SOCK and a real model wired
# (MODEL_READY=1). If the apollia-runner sidecar is unreachable (common in a
# sub-shell / CI), the model-bound captures degrade to a justified skip, never a
# hard failure. Timing per capture comes from run_capture.py / pty_run.py.

Q=("$BIN" --socket "$SOCK")

# The model is boot-loaded and the orchestrator already waited for readiness, so
# this is a fast confirmation. An unreachable runner degrades to a justified skip.
section "C.0 runner reachability"
if "${Q[@]}" llm ping local-qwen >/dev/null 2>&1; then
    _pass "llm ping local-qwen (runner reachable)"
    RUNNER_OK=1
else
    skip "Track 3 model-bound captures" "apollia-runner sidecar unreachable in this env (llm ping failed)"
    RUNNER_OK=0
fi

if [[ "$RUNNER_OK" == "1" ]]; then
    # ── C.1 llm chat (one-shot, non-deterministic) ──────────────────────────
    section "C.1 llm chat (capture)"
    capture_run "llm chat one-shot" 120 "Reply with the single word OK." -- \
        "${Q[@]}" llm chat "Reply with the single word OK." --backend local-qwen

    # ── C.2 local-model meta commands (runner-dependent → soft) ─────────────
    # Run while the runner is fresh; skip (not fail) if the sidecar drops.
    section "C.2 do / explain (local model)"
    capture_run_soft "do (NL → command)" 120 "list the installed agents" -- \
        "${Q[@]}" do "list the installed agents" -y
    capture_run_soft "explain (command/error)" 120 "explain exit code 2" -- \
        "${Q[@]}" explain "what does exit code 2 mean for apollia-os"

    # ── C.3 run --stream (SSE token stream) ─────────────────────────────────
    section "C.3 run --stream (capture)"
    # Reinstall + start the echo stub so `run --stream` has a running target
    # that streams without depending on model quality.
    RUN_STUB="$RUN_TMP/hello3.py"
    /bin/cp "$RUN_TMP/hello.py" "$RUN_STUB" 2>/dev/null || true
    if [[ -f "$RUN_STUB" ]] && "${Q[@]}" agent install "$RUN_STUB" --skip-tests >/dev/null 2>&1 \
        && "${Q[@]}" agent start e2e-hello >/dev/null 2>&1; then
        capture_stream "run --stream (echo stub)" 90 1 "ping" -- \
            "${Q[@]}" run e2e-hello "ping" --stream
        "${Q[@]}" agent uninstall e2e-hello >/dev/null 2>&1 || true
    else
        skip "run --stream" "echo stub could not be started (Python runner)"
    fi

    # ── C.4 chat REPL streaming (the marquee test: prove the stream) ─────────
    # Runs last: it is the longest capture. Pass criterion is that a reply
    # STREAMED (multiple progressive chunks), not that the REPL exits cleanly
    # (we drive it and terminate it via the pty on purpose).
    section "C.4 chat REPL streaming (pty capture)"
    capture_pty "chat REPL streams a reply" 120 3 "In one short sentence, what is Apollia OS?" -- \
        "${Q[@]}" chat

    # ── C.5 review (opt-in, heavy) ──────────────────────────────────────────
    section "C.5 review (opt-in)"
    if [[ "${TEST_REVIEW:-0}" == "1" ]]; then
        capture_run_soft "review ." 600 "review the working tree" -- "${Q[@]}" review .
    else
        skip "review ." "APOLLIA_TEST_REVIEW=0 (opt-in, several minutes)"
    fi
fi

# ── C.6 other non-deterministic surfaces (no local model needed) ────────────
# seed-classifier auto-starts and exposes classify_text, so capture a real A2A
# invocation for human review (structure asserted, label choice not).
section "C.6 other non-deterministic surfaces"
capture_run_soft "a2a invoke classify_text" 90 '{"text":"the app crashes on login","labels":["bug","feature"]}' -- \
    "${Q[@]}" a2a invoke classify_text --args '{"text":"the app crashes on login","labels":["bug","feature"]}' --timeout 60
skip "audit replay <run>" "requires a completed run in the journal to replay"
