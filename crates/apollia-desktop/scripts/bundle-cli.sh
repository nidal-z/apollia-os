#!/usr/bin/env bash
# Staging script invoked by Tauri's `beforeBuildCommand` (tauri.conf.json).
#
# Responsibilities:
#   1. Build the Python bundle (python-build-standalone + bundled deps) for the
#      target triple and stage it at crates/apollia-desktop/resources/python/.
#   2. Build the apollia-cli release binary linked against the bundled libpython
#      (via PYO3_PYTHON) and stage it at crates/apollia-desktop/resources/apollia-os.
#   3. Build the Svelte frontend (ui/ → ui/dist/).
#
# The Tauri build (cargo tauri build) runs AFTER this script, so the desktop
# binary (apollia-desktop) is built separately by Tauri itself - we export
# PYO3_PYTHON so both binaries link against the same libpython.
#
# Post-build Mach-O / ELF patching of apollia-desktop is done by Tauri's
# `afterBundleCommand` hook (see tauri.conf.json); this script only handles
# the pre-build staging.
set -euo pipefail

TARGET="${TAURI_TARGET_TRIPLE:-$(rustc -vV | grep host | awk '{print $2}')}"
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

export PYO3_PYTHON="${STAGING}/python/bin/python3.13"
export PYTHONHOME="${STAGING}/python"
# python-build-standalone has a hardcoded /install/lib LIBDIR that PyO3 picks up.
# Override the library search path to point at the actual bundled libpython.
export RUSTFLAGS="${RUSTFLAGS:-} -L ${STAGING}/python/lib"
echo "==> PYO3_PYTHON=${PYO3_PYTHON}"
echo "==> RUSTFLAGS=${RUSTFLAGS}"

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
        cp "${REPO_ROOT}/target/${TARGET}/release/apollia-os" "${STAGING}/apollia-os"
        ;;
esac

echo "==> CLI binary staged at ${STAGING}/apollia-os"

# ── Step 2b - Runners (ADR-113 sidecars) ──────────────────────────────────────
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
    echo "    staged $(basename "$dst")"
done

# ── Step 3 - Frontend ─────────────────────────────────────────────────────────

echo "==> Building Svelte frontend..."
cd "${DESKTOP_DIR}/ui" && npm run build
echo "==> Done."
