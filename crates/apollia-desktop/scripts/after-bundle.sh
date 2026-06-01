#!/usr/bin/env bash
# Tauri `afterBundleCommand` hook - runs AFTER the app bundle is assembled
# (Contents/MacOS/apollia-desktop exists, Contents/Resources/python/ is in place).
#
# Responsibilities:
#   1. Rewrite the apollia-desktop Mach-O's install_name references to libpython
#      so it resolves via @executable_path/../Resources/python/lib/… at runtime,
#      never against the dev machine's Homebrew Python.
#   2. Same treatment for apollia-os (the CLI) in Contents/Resources/.
#   3. On Linux: patch the ELF's RPATH with `patchelf --set-rpath $ORIGIN/...`.
#
# The script discovers the bundle layout from Tauri's env vars when available,
# falls back to the conventional path under target/ otherwise.
set -euo pipefail

TARGET="${TAURI_TARGET_TRIPLE:-$(rustc -vV | grep host | awk '{print $2}')}"
REPO_ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"

# ── Locate the bundle ─────────────────────────────────────────────────────────

case "$TARGET" in
    *-apple-darwin|universal-apple-darwin)
        BUNDLE_DIR=$(find "${REPO_ROOT}/target/${TARGET}/release/bundle/macos" \
            -maxdepth 1 -name "*.app" -print -quit 2>/dev/null || true)
        if [[ -z "$BUNDLE_DIR" ]]; then
            echo "==> no .app bundle found - skipping post-bundle patch"
            exit 0
        fi
        APP_BINARY="${BUNDLE_DIR}/Contents/MacOS/apollia-desktop"
        CLI_BINARY="${BUNDLE_DIR}/Contents/Resources/apollia-os"
        PYTHON_LIB_REL="@executable_path/../Resources/python/lib/libpython3.13.dylib"
        patch_macos_binary() {
            local bin="$1"
            # Find the current libpython install_name reference and replace it.
            local current
            current=$(otool -L "$bin" 2>/dev/null | awk '/libpython3\.13/ {print $1; exit}' || true)
            if [[ -z "$current" ]]; then
                echo "    $bin: no libpython3.13 reference - skipping"
                return
            fi
            if [[ "$current" == "$PYTHON_LIB_REL" ]]; then
                echo "    $bin: already patched"
                return
            fi
            echo "    $bin: $current → $PYTHON_LIB_REL"
            install_name_tool -change "$current" "$PYTHON_LIB_REL" "$bin"
        }
        echo "==> patching Mach-O install_names..."
        patch_macos_binary "$APP_BINARY"
        [[ -f "$CLI_BINARY" ]] && patch_macos_binary "$CLI_BINARY"
        ;;

    *-unknown-linux-gnu)
        # Tauri produces AppImage + .deb - we patch the binary before Tauri wraps
        # it. By this point the app binary lives in target/release/apollia-desktop
        # and the CLI in crates/apollia-desktop/resources/apollia-os.
        APP_BINARY="${REPO_ROOT}/target/${TARGET}/release/apollia-desktop"
        CLI_BINARY="${REPO_ROOT}/crates/apollia-desktop/resources/apollia-os"
        # Relative to the binary's location inside the AppImage squashfs:
        #   usr/bin/apollia-desktop        → ../lib/apollia-os/python/lib
        #   usr/lib/apollia-os/apollia-os  → ../../lib/apollia-os/python/lib (same tree)
        PYTHON_RPATH='$ORIGIN/../lib/apollia-os/python/lib:$ORIGIN/../Resources/python/lib'
        if ! command -v patchelf >/dev/null 2>&1; then
            echo "==> patchelf not found - install via apt/dnf and re-run"
            exit 2
        fi
        echo "==> patchelf --set-rpath '${PYTHON_RPATH}' on ELF binaries..."
        for bin in "$APP_BINARY" "$CLI_BINARY"; do
            [[ -f "$bin" ]] || continue
            patchelf --set-rpath "$PYTHON_RPATH" "$bin"
        done
        ;;

    *)
        echo "==> unsupported target '$TARGET' - skipping"
        exit 0
        ;;
esac

echo "==> post-bundle patching done"
