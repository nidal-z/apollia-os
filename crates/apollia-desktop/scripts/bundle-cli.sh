#!/usr/bin/env bash
# Staging script invoked by Tauri's `beforeBuildCommand` (tauri.conf.json).
#
# Responsibilities:
#   1. Build the Python bundle (python-build-standalone + bundled deps) for the
#      target triple and stage it at crates/apollia-desktop/python/.
#   2. Build the apollia-cli release binary linked against the bundled libpython
#      (via PYO3_PYTHON) and stage it at crates/apollia-desktop/apollia-os.
#   3. Build the Svelte frontend (ui/ → ui/dist/).
#
# The Tauri build (cargo tauri build) runs AFTER this script, so the desktop
# binary (apollia-desktop) is built separately by Tauri itself - we export
# PYO3_PYTHON so both binaries link against the same libpython.
#
# Pre-bundle Mach-O / ELF patching of apollia-desktop is done by Tauri's
# `beforeBundleCommand` hook (scripts/patch-prebundle-libpython.sh); this
# script only handles the pre-build staging.
set -euo pipefail

TARGET="${TAURI_ENV_TARGET_TRIPLE:-$(rustc -vV | grep host | awk '{print $2}')}"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
DESKTOP_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
REPO_ROOT="$(cd "${DESKTOP_DIR}/../.." && pwd)"
# Stage directly in crates/apollia-desktop/ (not resources/) so Tauri copies to
# Contents/Resources/ (not Contents/Resources/resources/)
STAGING="${DESKTOP_DIR}"

mkdir -p "$STAGING"

# ── Step 1 - Python bundle ────────────────────────────────────────────────────

echo "==> Building Python bundle for target ${TARGET}..."
"${REPO_ROOT}/packaging/build-python-bundle.sh" "$TARGET" "${REPO_ROOT}/target/python-bundle/${TARGET}"

# Copy (or re-copy) the result into resources/python/ - overwrite freely, Tauri
# will pick up whatever's here.
rm -rf "${STAGING}/python"
cp -R "${REPO_ROOT}/target/python-bundle/${TARGET}/python" "${STAGING}/python"
# `python/.gitkeep` is tracked, and it is what makes the `python/**/*` resource
# glob of tauri.conf.json match in a tree where no bundle has been staged yet.
# The rm above deletes it, which would leave the working tree dirty after every
# desktop build. Same role runners/README.txt plays for `runners/**/*`.
touch "${STAGING}/python/.gitkeep"

# Executable suffix and Python layout, both target-dependent. Computed here
# because the CLI copy below needs the suffix, and the previous code only derived
# it further down for the runners, leaving the CLI copied without `.exe`.
BIN_EXT=""
case "$TARGET" in
    *-pc-windows-*) BIN_EXT=".exe" ;;
esac

# python-build-standalone ships a flat layout on Windows (`python.exe` at the
# root, import libraries under `libs/`) and a POSIX one elsewhere
# (`bin/python3.13`, shared library under `lib/`). Exporting the POSIX shape
# unconditionally pointed PyO3 at a path that does not exist on Windows, and
# overrode the correct value the CI job had already set.
if [ -n "$BIN_EXT" ]; then
    export PYO3_PYTHON="${STAGING}/python/python.exe"
    PY_LIB_DIR="${STAGING}/python/libs"
else
    export PYO3_PYTHON="${STAGING}/python/bin/python3.13"
    PY_LIB_DIR="${STAGING}/python/lib"
fi
export PYTHONHOME="${STAGING}/python"
# python-build-standalone has a hardcoded /install/lib LIBDIR that PyO3 picks up.
# Override the library search path to point at the actual bundled libpython.
export RUSTFLAGS="${RUSTFLAGS:-} -L ${PY_LIB_DIR}"
echo "==> PYO3_PYTHON=${PYO3_PYTHON}"
echo "==> RUSTFLAGS=${RUSTFLAGS}"

if [ ! -e "$PYO3_PYTHON" ]; then
    echo "==> ERROR: bundled interpreter not found at ${PYO3_PYTHON}." >&2
    echo "    The Python bundle for ${TARGET} did not produce the expected layout." >&2
    exit 1
fi

# ── Step 2 - CLI binary (apollia-os) ──────────────────────────────────────────

echo "==> Building apollia-cli for target ${TARGET}..."
# For universal2 on macOS, Tauri cargo-tauri runs cargo with `--target universal-apple-darwin`
# which is NOT a real target - we need to build both arches and lipo them.
case "$TARGET" in
    universal-apple-darwin)
        for arch_triple in aarch64-apple-darwin x86_64-apple-darwin; do
            echo "  -> $arch_triple"
            # Each arch needs its own PYO3_PYTHON pointing at the lipo'd universal
            # Python - same file, since the universal Python dylib contains both.
            cargo build -p apollia-cli --release \
                --target "$arch_triple" \
                --manifest-path "${REPO_ROOT}/Cargo.toml"
        done
        mkdir -p "${REPO_ROOT}/target/universal-apple-darwin/release"
        lipo -create \
            "${REPO_ROOT}/target/aarch64-apple-darwin/release/apollia-os" \
            "${REPO_ROOT}/target/x86_64-apple-darwin/release/apollia-os" \
            -output "${REPO_ROOT}/target/universal-apple-darwin/release/apollia-os"
        cp "${REPO_ROOT}/target/universal-apple-darwin/release/apollia-os" "${STAGING}/apollia-os"
        ;;
    *)
        cargo build -p apollia-cli --release \
            --target "$TARGET" \
            --manifest-path "${REPO_ROOT}/Cargo.toml"
        cp "${REPO_ROOT}/target/${TARGET}/release/apollia-os${BIN_EXT}" \
           "${STAGING}/apollia-os${BIN_EXT}"
        ;;
esac

echo "==> CLI binary staged at ${STAGING}/apollia-os${BIN_EXT}"

# ── Step 2b - Runners (sidecars) ──────────────────────────────────────
# `APOLLIA_DESKTOP_RUNNERS` selects which runner backends to build and stage
# alongside the daemon. Examples :
#   APOLLIA_DESKTOP_RUNNERS=cpu                # CPU only (minimal)
#   APOLLIA_DESKTOP_RUNNERS="cpu cuda vulkan"  # Windows/Linux full set
#   APOLLIA_DESKTOP_RUNNERS="cpu metal"        # macOS full set
# Defaults to "cpu" when unset, guaranteeing universal fallback. CI overrides
# this per platform with the right backends + SDK install steps.
RUNNERS="${APOLLIA_DESKTOP_RUNNERS:-cpu}"

# Pour la cible universal macOS, on délègue à `aarch64-apple-darwin` (le runner
# Apple Silicon couvre la cible Apollia v0.1.0 ; x86_64 macOS sera fait via
# Rosetta côté daemon mais le runner reste arm64 pour Metal/perf).
RUNNER_TARGET="$TARGET"
if [ "$RUNNER_TARGET" = "universal-apple-darwin" ]; then
    RUNNER_TARGET="aarch64-apple-darwin"
fi

# Détermine l'extension du binaire (.exe sur Windows).
BIN_EXT=""
case "$RUNNER_TARGET" in
    *-pc-windows-*) BIN_EXT=".exe" ;;
esac

# Runners staging directory: tauri.conf.json déclare `runners/**` comme
# resource. Le `.gitkeep` reste pour que le glob matche même quand aucun
# runner n'a encore été buildé (cargo check pur, dev local sans bundle).
mkdir -p "${STAGING}/runners"
rm -f "${STAGING}/runners/apollia-runner-"*

for backend in $RUNNERS; do
    feature="local-${backend}"
    echo "==> Building apollia-runner (${feature}) for ${RUNNER_TARGET}..."
    cargo build -p apollia-runner --release \
        --no-default-features \
        --features "${feature}" \
        --target "${RUNNER_TARGET}" \
        --manifest-path "${REPO_ROOT}/Cargo.toml"

    src="${REPO_ROOT}/target/${RUNNER_TARGET}/release/apollia-runner${BIN_EXT}"
    dst="${STAGING}/runners/apollia-runner-${backend}${BIN_EXT}"
    cp "$src" "$dst"
    chmod +x "$dst" || true
    # On macOS, `cp` invalidates the linker-signed adhoc signature, so the
    # kernel SIGKILLs the staged runner at launch ("Code Signature Invalid").
    # Re-sign adhoc here; the bundle is re-signed as a whole by Tauri afterwards.
    case "$RUNNER_TARGET" in
        *-apple-darwin) codesign --force --sign - "$dst" ;;
    esac
    echo "    staged $(basename "$dst")"
done

# ── Step 2c - Embedded llama-server (local LLM engine) ────────────────
# Bundle the upstream llama-server next to the STT runners; the daemon's
# `locate_llama_server_binary` finds it there. Pick the GPU backend from
# RUNNERS (metal/cuda/rocm/vulkan), else cpu.
#
# FATAL on failure, deliberately. This script is the `beforeBuildCommand` of
# `tauri build` and never runs in dev, so every invocation produces a bundle
# meant for distribution. A bundle without the engine still *works on the
# machine that built it*, because the locator falls back to a llama-server on
# PATH, and then has no local inference at all for everyone else. That is the
# worst possible failure: invisible where it is built, total where it is used.
#
# Escape hatches, both explicit: LLAMA_SERVER_DIR=<bin dir> bundles a local
# build instead of downloading, and APOLLIA_ALLOW_NO_LLAMA_SERVER=1 accepts a
# bundle with no engine (offline work on a non-inference change only).
llama_backend="cpu"
for backend in $RUNNERS; do
    case "$backend" in
        metal | cuda | rocm | vulkan)
            llama_backend="$backend"
            break
            ;;
    esac
done
if bash "${REPO_ROOT}/packaging/fetch-llama-server.sh" "$llama_backend" "${STAGING}/runners"; then
    echo "==> llama-server bundled (${llama_backend})"
elif [ "${APOLLIA_ALLOW_NO_LLAMA_SERVER:-}" = "1" ]; then
    echo "==> WARNING: APOLLIA_ALLOW_NO_LLAMA_SERVER=1, bundling without a local" >&2
    echo "    LLM engine. This bundle has no local inference off this machine." >&2
else
    echo "==> ERROR: could not bundle llama-server (${llama_backend})." >&2
    echo "    A bundle without the engine has no local inference for anyone but" >&2
    echo "    the machine that built it. Fix the fetch, or pass" >&2
    echo "    LLAMA_SERVER_DIR=<bin dir> to bundle a local build." >&2
    exit 1
fi

# ── Step 3 - Frontend ─────────────────────────────────────────────────────────

echo "==> Building Svelte frontend..."
cd "${DESKTOP_DIR}/ui" && npm run build
echo "==> Done."
