# shellcheck shell=bash
# tests/cli/lib/seed.sh - build a fresh, throwaway seeded HOME for the CLI E2E
# suite. Reuses the single shared seed builder (tests/cli/seed) rather
# than forking a second fixture set: the seed is the one source of truth for the
# deterministic Apollia data profile, shared with the desktop automation suite.
#
# The builder writes fixed ids and fixed 2026-07-01 timestamps into a full
# `.apollia` (all DBs + agents + memory + placeholder models + config), so the
# read-path commands assert KNOWN content instead of empty states. Every call
# produces a clean HOME (`rm -rf` inside the builder), so a run never mutates the
# reference fixtures and never touches the real ~/.apollia.

# Absolute path to the shared seed builder, resolved from the repo root.
_seed_builder() {
    printf '%s/tests/cli/seed/build-seed.sh' "$REPO_ROOT"
}

# build_seed_home <dest>
#   Build a deterministic seeded HOME at <dest>. Prints nothing on success;
#   returns non-zero (and a diagnostic) if the builder or its deps are missing.
build_seed_home() {
    local dest=$1
    local builder
    builder=$(_seed_builder)
    if [[ ! -f "$builder" ]]; then
        echo "seed builder not found at $builder" >&2
        return 1
    fi
    if ! command -v sqlite3 >/dev/null 2>&1; then
        echo "sqlite3 is required to build the seed fixture" >&2
        return 1
    fi
    # The builder derives the data dir from its argument and rebuilds it clean.
    #
    # The diagnostic carries the tail of the log rather than its path: the log
    # lives under $RUN_TMP, which the orchestrator's EXIT trap removes, so a
    # reader sent to the path finds nothing there.
    if ! bash "$builder" "$dest" >"$dest.buildlog" 2>&1; then
        echo "seed build failed; last lines of the builder log:" >&2
        /usr/bin/tail -20 "$dest.buildlog" >&2
        return 1
    fi
    return 0
}

# seed_wire_real_model <seed_home> <gguf>
#   Point the seeded default LLM backend at a real GGUF (by ABSOLUTE path) so
#   Track 3 can drive live inference, WITHOUT copying the model or symlinking the
#   real models dir. The runner opens the file read-only.
#
#   Do NOT use `llm setup --local --models-dir <real models dir>`: it copies the
#   source model into the target dir, and if the target resolves to the dir that
#   already holds the source (e.g. via a symlink to the real models dir), the
#   copy truncates the real model to 0 bytes. This is a copy-onto-itself hazard;
#   keep the wiring a pure metadata update against the throwaway seed's DB.
seed_wire_real_model() {
    local seed_home=$1 gguf=$2 sysdb
    [[ -f "$gguf" ]] || return 1
    sysdb="$seed_home/.apollia/system.db"
    [[ -f "$sysdb" ]] || return 1
    # Repoint the seeded default backend at the absolute real path (no copy,
    # read-only) and make it the sole boot default. The daemon builds its router
    # from this table at startup; a runtime `set-default` + `reload` refreshes the
    # router but does NOT spawn a local load, so writing the default before boot
    # is the only reliable path. The orchestrator then waits for readiness before
    # Track 2.
    #
    # The name must be one the seed actually writes. seed/fragments/system.sql
    # declares local-llama-server, openai-gpt4o-mini and anthropic-claude, and
    # nothing else: a WHERE on any other name matches no row, and the preceding
    # `SET is_default = 0` then leaves the base with NO default at all, a state
    # `reload_llm_from_db` refuses.
    /usr/bin/sqlite3 "$sysdb" \
        "UPDATE llm_backends SET is_default = 0;
         UPDATE llm_backends
            SET model = '$gguf',
                config_json = json_set(COALESCE(NULLIF(config_json,''),'{}'), '\$.model_path', '$gguf'),
                is_default = 1,
                enabled = 1
          WHERE name = 'local-llama-server';" >/dev/null 2>&1 || return 1
}

# seed_wait_model_ready [tries]  (daemon must be up on $SOCK)
#   Poll `llm ping local-llama-server`, the backend seed_wire_real_model just
#   repointed, until it answers. Returns 0 when ready. A multi-GB GGUF can take
#   tens of seconds to load after the socket is up, so both Track 2 and Track 3
#   wait here to stay race-free.
seed_wait_model_ready() {
    local tries=${1:-40} i=0
    while [[ $i -lt $tries ]]; do
        "$BIN" --socket "$SOCK" llm ping local-llama-server >/dev/null 2>&1 && return 0
        /bin/sleep 2; i=$((i + 1))
    done
    return 1
}
