#!/usr/bin/env bash
# Tauri `beforeBundleCommand` hook: runs AFTER `cargo build` produces the
# apollia-desktop binary but BEFORE Tauri assembles the .app and .dmg.
#
# It rewrites the binary's libpython reference to the bundled, relocatable path
# so that BOTH the .app and the .dmg embed a correct binary. Doing this
# post-bundle (a since-removed hook) was too late: `cargo tauri build` seals the .dmg
# from the freshly-assembled .app, so a post-bundle patch fixed the .app on disk
# but left the .dmg (the thing users install) pointing at the developer's
# absolute Python path, and every embedded Python agent failed to import.
#
# Tauri signs the .app afterwards (signingIdentity "-" + entitlements), so the
# patched binary ends up signed inside the .dmg.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"

# Locate the binary Tauri is about to bundle. Prefer the target-triple path
# Tauri exposes; fall back to the plain release dir, then to the newest match.
TARGET="${TAURI_ENV_TARGET_TRIPLE:-}"
BIN=""
if [ -n "$TARGET" ] && [ -f "${REPO_ROOT}/target/${TARGET}/release/apollia-desktop" ]; then
    BIN="${REPO_ROOT}/target/${TARGET}/release/apollia-desktop"
elif [ -f "${REPO_ROOT}/target/release/apollia-desktop" ]; then
    BIN="${REPO_ROOT}/target/release/apollia-desktop"
else
    BIN="$(ls -t "${REPO_ROOT}"/target/*/release/apollia-desktop 2>/dev/null | head -1 || true)"
fi

if [ -z "${BIN:-}" ] || [ ! -f "$BIN" ]; then
    echo "==> patch-prebundle-libpython: apollia-desktop binary not found, skip"
    exit 0
fi

case "$(uname)" in
    Darwin)
        NEW="@executable_path/../Resources/python/lib/libpython3.13.dylib"
        current="$(otool -L "$BIN" 2>/dev/null | awk '/libpython3\.13\.dylib/ {print $1; exit}' || true)"
        if [ -z "$current" ]; then
            echo "==> $BIN: no dynamic libpython reference (static?), skip"
        elif [ "$current" = "$NEW" ]; then
            echo "==> $BIN: libpython already relocatable"
        else
            echo "==> $BIN: rewriting libpython '$current' -> '$NEW'"
            install_name_tool -change "$current" "$NEW" "$BIN"
        fi
        ;;
    Linux)
        # AppImage/.deb: usr/bin/apollia-desktop -> usr/lib/apollia-os/python/lib
        if command -v patchelf >/dev/null 2>&1; then
            patchelf --set-rpath '$ORIGIN/../lib/apollia-os/python/lib' "$BIN" || true
            echo "==> $BIN: rpath set for bundled libpython"
        else
            echo "==> patchelf not available, skip Linux rpath patch"
        fi
        ;;
    *)
        echo "==> $(uname): no libpython relocation needed here"
        ;;
esac
