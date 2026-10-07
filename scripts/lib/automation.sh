# shellcheck shell=bash
# The desktop automaton recipes. Sourced by the justfile, which hands its
# settings over as AUTOMATION_OUT, SEED_HOME and the LLAMA_* shell variables.

AUTOMATION_LIB_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=python-bundle.sh
source "$AUTOMATION_LIB_DIR/python-bundle.sh"
# shellcheck source=llama.sh
source "$AUTOMATION_LIB_DIR/llama.sh"

# automation_run <home> <script> [model]
#
# Drive the dev build of the application with one script. <home> says which
# profile the application meets:
#
#   seeded    a throwaway HOME rebuilt from tests/cli/seed, the assertion
#             suites' fixture. The narrative overlay is stripped: it changes
#             row counts, and a suite run with it reads its own failures as
#             product regressions.
#   shooting  the same seed with the narrative overlay kept, for screenshots,
#             where an empty timeline is the defect.
#   blank     SEED_HOME as the caller prepared it, with no seed and with the
#             public MCP registry in reach, as a new installation has it.
#   real      the caller's own HOME.
#
# A third argument, even empty, asks for a background llama-server on that
# model, started with the automation profile. It loads the model from the real
# home: only the application's HOME is swapped.
automation_run() {
    local home_mode="$1" script="$2" model="${3:-}" with_model=0
    if [ "$#" -ge 3 ]; then with_model=1; fi
    if [ ! -f "$script" ]; then
        echo "error: script not found: $script (try scripts/automation/master-det.json)" >&2
        return 1
    fi
    if [ "$with_model" -eq 1 ]; then llama_require_model "$model" || return 1; fi
    if [ "$home_mode" = "shooting" ] && [ ! -d "${APOLLIA_SEED_OVERLAY:-}" ]; then
        echo "error: narrative overlay not found at '${APOLLIA_SEED_OVERLAY:-}'." >&2
        echo "       It lives outside the repository. Set APOLLIA_SEED_OVERLAY to its" >&2
        echo "       directory, or run automation-seeded and accept empty timelines." >&2
        return 1
    fi

    local repo script_abs out seed_home
    repo="$(cd "$AUTOMATION_LIB_DIR/../.." && pwd)"
    script_abs="$(cd "$(dirname "$script")" && pwd)/$(basename "$script")"
    out="${AUTOMATION_OUT:-$PWD/.apollia-automation}"
    mkdir -p "$out"
    out="$(cd "$out" && pwd)"
    seed_home="${SEED_HOME:-$PWD/.apollia-seed-home}"

    case "$home_mode" in
        seeded) env -u APOLLIA_SEED_OVERLAY bash "$repo/tests/cli/seed/build-seed.sh" "$seed_home" || return 1 ;;
        shooting) bash "$repo/tests/cli/seed/build-seed.sh" "$seed_home" || return 1 ;;
        blank) echo "-> blank home, no seed: $seed_home" ;;
        real) ;;
        *)
            echo "error: unknown automation home '$home_mode'" >&2
            return 1
            ;;
    esac
    case "$home_mode" in
        seeded | shooting)
            # A seeded run answers from the seeded MCP registry cache, never
            # from the public registry: the cache goes stale after fifteen
            # minutes, and a long book would otherwise swap the two seeded
            # servers for the public ones mid-run, on network timing alone.
            export APOLLIA_MCP_REGISTRY_URL="http://127.0.0.1:1"
            ;;
    esac

    # The toolchain defaults derive from the real home: pin them before the
    # application's HOME is swapped, so cargo and rustc still resolve.
    export CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}"
    export RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}"
    python_bundle_activate || return 1
    if [ "$with_model" -eq 1 ]; then llama_start "$model" automation || return 1; fi

    echo "-> automation ($home_mode home): $script (out: $out)"
    cd "$repo/crates/apollia-desktop" || return 1
    if [ "$home_mode" = "real" ]; then
        APOLLIA_AUTOMATION="$script_abs" APOLLIA_AUTOMATION_OUT="$out" \
            RUST_LOG=debug cargo tauri dev
    else
        HOME="$seed_home" USERPROFILE="$seed_home" \
            APOLLIA_AUTOMATION="$script_abs" APOLLIA_AUTOMATION_OUT="$out" \
            RUST_LOG=info cargo tauri dev
    fi
}

# automation_blank_home <model> [embeddings]
#
# Build the home a first launch meets, with one thing in it: the two model
# files, so the run measures the launch and not a download. Echoes its path.
#
# The files are cloned, not linked. The scan of the models folder skips a
# symbolic link, so a link would show no model; a hard link would let a write
# through to the real file; a copy-on-write clone (APFS, or a reflink where the
# file system has one) costs no bytes and leaves the real file out of reach.
# Elsewhere the files are copied. The embeddings file keeps its registry name,
# which is how the embeddings step of a first launch recognises it.
automation_blank_home() {
    local model="${1:-}" embeddings="${2:-}" file
    local name="multilingual-e5-small-q8_0.gguf"
    # A fixed path, so the removal below can never be pointed elsewhere.
    local home="$PWD/.apollia-first-run-home"
    llama_require_model "$model" || return 1
    [ -n "$embeddings" ] || embeddings="$HOME/.apollia/models/$name"
    if [ ! -f "$embeddings" ] || [ "$(basename "$embeddings")" != "$name" ]; then
        echo "error: embeddings model not found: $embeddings" >&2
        echo "       A first launch imports $name, under that name. The embeddings" >&2
        echo "       step of a first launch downloads it once, and the Hub repository" >&2
        echo "       cstr/multilingual-e5-small-GGUF holds the file. Pass its path as" >&2
        echo "       the third argument, or leave it in the models folder of your home." >&2
        return 1
    fi
    rm -rf "$home"
    mkdir -p "$home/.apollia/models"
    for file in "$model" "$embeddings"; do
        case "$(uname -s)" in
            Darwin) cp -c "$file" "$home/.apollia/models/" ;;
            Linux) cp --reflink=auto "$file" "$home/.apollia/models/" ;;
            *) cp "$file" "$home/.apollia/models/" ;;
        esac
    done
    echo "-> first launch on a blank home: $(basename "$model") + $name" >&2
    printf '%s\n' "$home"
}
