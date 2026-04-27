#!/usr/bin/env bash
# Reproduce the GitHub Actions CI build pipeline for macOS universal2.
#
# This script mirrors `.github/workflows/build-desktop.yml` (build-macos job)
# to allow local validation before pushing to CI.
#
# Usage:
#   ./scripts/test-build-macos.sh [OPTIONS]
#
# Options:
#   --clean              Clean build artifacts before starting
#   --skip-python        Skip Python bundle build (use existing bundle)
#   --skip-frontend      Skip npm ci (use existing node_modules)
#   --skip-codesign      Skip ad-hoc codesigning step
#   --verbose            Enable verbose output (set -x)
#   -h, --help           Show this help
#
# Environment variables (same as CI):
#   CARGO_TERM_COLOR=always
#   RUST_BACKTRACE=1
#   PYO3_PYTHON=<path to bundled python3.13>
#
set -euo pipefail

# ── Configuration ────────────────────────────────────────────────────────────
PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TARGET_TRIPLE="universal-apple-darwin"
PYTHON_BUNDLE_DIR="${PROJECT_ROOT}/target/python-bundle/${TARGET_TRIPLE}"
PYO3_PYTHON="${PYTHON_BUNDLE_DIR}/python/bin/python3.13"

export CARGO_TERM_COLOR=always
export RUST_BACKTRACE=1

# ── Parse arguments ──────────────────────────────────────────────────────────
CLEAN=false
SKIP_PYTHON=false
SKIP_FRONTEND=false
SKIP_CODESIGN=false
VERBOSE=false

while [[ $# -gt 0 ]]; do
    case "$1" in
        --clean)         CLEAN=true; shift ;;
        --skip-python)   SKIP_PYTHON=true; shift ;;
        --skip-frontend) SKIP_FRONTEND=true; shift ;;
        --skip-codesign) SKIP_CODESIGN=true; shift ;;
        --verbose)       VERBOSE=true; shift ;;
        -h|--help)
            sed -n '2,/^$/p' "$0" | grep '^#' | sed 's/^# \?//'
            exit 0
            ;;
        *)
            echo "error: unknown option '$1'" >&2
            echo "Run '$0 --help' for usage." >&2
            exit 2
            ;;
    esac
done

if $VERBOSE; then
    set -x
fi

cd "$PROJECT_ROOT"

# ── Step 0: Prerequisites check ──────────────────────────────────────────────
echo "==> Checking prerequisites"

if ! command -v rustc &>/dev/null; then
    echo "error: rustc not found. Install via https://rustup.rs" >&2
    exit 1
fi

if ! command -v cargo &>/dev/null; then
    echo "error: cargo not found" >&2
    exit 1
fi

# Check Rust targets (aarch64-apple-darwin, x86_64-apple-darwin)
if command -v rustup &>/dev/null; then
    for target in aarch64-apple-darwin x86_64-apple-darwin; do
        if ! rustup target list --installed | grep -q "^${target}$"; then
            echo "    installing missing target: $target"
            rustup target add "$target"
        fi
    done
else
    echo "    ⚠ rustup not found — skipping target verification"
    echo "      If the build fails, install targets manually or use rustup."
    echo "      See: https://rustup.rs"
fi

if ! command -v node &>/dev/null; then
    echo "error: node not found. Install Node.js 22+ (https://nodejs.org)" >&2
    exit 1
fi

NODE_VERSION=$(node --version | sed 's/v//' | cut -d. -f1)
if [[ "$NODE_VERSION" -lt 18 ]]; then
    echo "warning: Node.js $NODE_VERSION detected, CI uses 22. Consider upgrading." >&2
fi

if ! command -v cargo-tauri &>/dev/null; then
    echo "    installing tauri-cli (this may take a few minutes)..."
    cargo install tauri-cli --version "^2" --locked
fi

echo "    ✓ Rust $(rustc --version | cut -d' ' -f2)"
echo "    ✓ Cargo $(cargo --version | cut -d' ' -f2)"
echo "    ✓ Node.js $(node --version)"
echo "    ✓ npm $(npm --version)"
echo "    ✓ Tauri CLI $(cargo tauri --version | head -1)"

# ── Step 1: Clean (optional) ─────────────────────────────────────────────────
if $CLEAN; then
    echo "==> Cleaning build artifacts"
    cargo clean
    rm -rf "${PROJECT_ROOT}/target/python-bundle"
    rm -rf "${PROJECT_ROOT}/crates/apollia-desktop/ui/node_modules"
    echo "    ✓ Cleaned target/ and node_modules"
fi

# ── Step 2: Install frontend dependencies ───────────────────────────────────
if $SKIP_FRONTEND; then
    echo "==> Skipping frontend dependencies (--skip-frontend)"
else
    echo "==> Installing frontend dependencies"
    cd "${PROJECT_ROOT}/crates/apollia-desktop/ui"
    npm ci
    echo "    ✓ npm ci completed"
    cd "$PROJECT_ROOT"
fi

# ── Step 3: Pre-build Python bundle (universal2) ─────────────────────────────
if $SKIP_PYTHON; then
    echo "==> Skipping Python bundle build (--skip-python)"
    if [[ ! -x "$PYO3_PYTHON" ]]; then
        echo "error: $PYO3_PYTHON not found. Remove --skip-python or build manually." >&2
        exit 1
    fi
else
    echo "==> Pre-build Python bundle (universal2)"
    ./packaging/build-python-bundle.sh "$TARGET_TRIPLE" "$PYTHON_BUNDLE_DIR"
    echo "    ✓ Python bundle ready at $PYTHON_BUNDLE_DIR"
fi

# ── Step 4: Build Tauri app (universal2) ─────────────────────────────────────
echo "==> Building Tauri app (universal2)"
cd "${PROJECT_ROOT}/crates/apollia-desktop"

export PYO3_PYTHON
echo "    PYO3_PYTHON=$PYO3_PYTHON"

cargo tauri build --target "$TARGET_TRIPLE"
echo "    ✓ Tauri build completed"

cd "$PROJECT_ROOT"

# ── Step 5: Post-bundle — patch install_names + ad-hoc sign ─────────────────
if $SKIP_CODESIGN; then
    echo "==> Skipping codesigning step (--skip-codesign)"
else
    echo "==> Post-bundle — patch install_names + ad-hoc sign (ADR-073)"
    cd "${PROJECT_ROOT}/crates/apollia-desktop"

    export TAURI_TARGET_TRIPLE="$TARGET_TRIPLE"
    bash scripts/after-bundle.sh

    APP="${PROJECT_ROOT}/target/${TARGET_TRIPLE}/release/bundle/macos/Apollia OS.app"
    if [[ ! -d "$APP" ]]; then
        echo "error: .app bundle not found at $APP" >&2
        exit 1
    fi

    codesign --force --deep --sign - \
        --options runtime \
        --entitlements entitlements.plist \
        "$APP"

    echo "    ==> Signature verification"
    codesign --verify --verbose=2 "$APP" || true
    echo "    ✓ Codesigning completed"

    cd "$PROJECT_ROOT"
fi

# ── Step 6: Generate SHA256SUMS ──────────────────────────────────────────────
echo "==> Generating SHA256SUMS"
DMG_DIR="${PROJECT_ROOT}/target/${TARGET_TRIPLE}/release/bundle/dmg"
if [[ -d "$DMG_DIR" ]]; then
    cd "$DMG_DIR"
    shasum -a 256 *.dmg > SHA256SUMS 2>/dev/null || true
    echo "    ✓ SHA256SUMS written to $DMG_DIR/SHA256SUMS"
    cd "$PROJECT_ROOT"
else
    echo "    ⚠ No DMG directory found (expected at $DMG_DIR)"
fi

# ── Summary ──────────────────────────────────────────────────────────────────
echo ""
echo "╔══════════════════════════════════════════════════════════════════════════╗"
echo "║                         BUILD SUCCESSFUL ✓                               ║"
echo "╚══════════════════════════════════════════════════════════════════════════╝"
echo ""
echo "Artifacts produced (same as CI):"
echo ""

APP="${PROJECT_ROOT}/target/${TARGET_TRIPLE}/release/bundle/macos/Apollia OS.app"
if [[ -d "$APP" ]]; then
    APP_SIZE=$(du -sh "$APP" | cut -f1)
    echo "  • .app bundle   : $APP ($APP_SIZE)"
fi

if [[ -d "$DMG_DIR" ]]; then
    for dmg in "$DMG_DIR"/*.dmg; do
        if [[ -f "$dmg" ]]; then
            DMG_SIZE=$(du -sh "$dmg" | cut -f1)
            DMG_NAME=$(basename "$dmg")
            echo "  • DMG           : $dmg ($DMG_SIZE)"
        fi
    done
    if [[ -f "$DMG_DIR/SHA256SUMS" ]]; then
        echo "  • SHA256SUMS    : $DMG_DIR/SHA256SUMS"
        echo ""
        echo "    Checksums:"
        sed 's/^/      /' "$DMG_DIR/SHA256SUMS"
    fi
fi

echo ""
echo "To test the .app bundle locally:"
echo "  open \"$APP\""
echo ""
echo "To install the DMG:"
if [[ -f "$DMG_DIR"/*.dmg ]]; then
    echo "  open \"$DMG_DIR\"/*.dmg"
fi
echo ""
