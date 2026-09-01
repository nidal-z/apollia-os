#!/usr/bin/env bash
# Orchestrate the full bundled-Python build:
#   1. Fetch python-build-standalone for the target triple (cached).
#   2. Install packaging/requirements-bundled.txt into the bundle's site-packages.
#   3. Prune unnecessary files (tests, __pycache__, .pyc, .dist-info minus METADATA).
#   4. Rewrite dylib install names / RPATH so the binary finds libpython
#      relative to its parent executable at runtime.
#
# Usage:
#   ./build-python-bundle.sh <target-triple> <output-dir>
#
# After success, <output-dir>/python/ is ready to be copied into a Tauri bundle
# as `Contents/Resources/python/` (macOS) or `usr/lib/apollia-os/python/` (Linux).
set -euo pipefail

# Isolate the bundle build from the developer's per-user site-packages
# (`~/.local/lib/python3.x/site-packages`). Without this, the standalone
# interpreter honours the user-site, so pip both reports the user's unrelated
# packages in its consistency check (a noisy but harmless warning) and, worse,
# can consider a bundled requirement "already satisfied" from the user-site and
# skip installing it into the bundle, shipping an incomplete site-packages. The
# bundle must contain exactly what requirements-bundled.txt + the SDK declare.
export PYTHONNOUSERSITE=1

TARGET="${1:?usage: build-python-bundle.sh <target-triple> <output-dir>}"
OUT_DIR="${2:?usage: build-python-bundle.sh <target-triple> <output-dir>}"

PACKAGING_DIR="$(cd "$(dirname "$0")" && pwd)"
REQUIREMENTS="${PACKAGING_DIR}/requirements-bundled.txt"

echo "==> Step 1/4: fetch python-build-standalone"
"${PACKAGING_DIR}/fetch-python-standalone.sh" "$TARGET" "$OUT_DIR"

PYTHON_DIR="${OUT_DIR}/python"

# The layout differs between POSIX and Windows:
#   POSIX   → python/bin/python3.13 (+ python/lib/python3.13/site-packages)
#   Windows → python/python.exe     (+ python/Lib/site-packages, flat layout)
if [[ -x "${PYTHON_DIR}/bin/python3.13" ]]; then
    PYTHON_BIN="${PYTHON_DIR}/bin/python3.13"
elif [[ -f "${PYTHON_DIR}/python.exe" ]]; then
    PYTHON_BIN="${PYTHON_DIR}/python.exe"
else
    echo "error: could not locate python interpreter in $PYTHON_DIR" >&2
    echo "       (looked for bin/python3.13 and python.exe)" >&2
    exit 2
fi

echo "==> Step 2/4: install bundled requirements"
# Use --no-compile to avoid burning disk on .pyc (re-generated at first import).
# Use --no-cache-dir to keep the build cache out of the distributed bundle.
# pip itself is pinned: an unpinned `--upgrade pip` changes the installer under
# the build from one run to the next, which is the opposite of a reproducible
# bundle. Bump the pin deliberately, with the requirements file.
"$PYTHON_BIN" -m pip install --no-cache-dir --no-compile pip==26.2.1
# --require-hashes: every wheel is checked against the sums pinned in
# requirements-bundled.txt, so a compromised index or mirror cannot slip a
# different artifact into the bundle. A missing or wrong sum is a hard error.
"$PYTHON_BIN" -m pip install --no-cache-dir --no-compile --require-hashes -r "$REQUIREMENTS"
# The Apollia SDK itself: agents import `apollia`, so the package must live in
# the bundled site-packages. Without this, every Python agent (onboarding,
# guide, chat) fails to load with a ModuleNotFoundError surfaced to the user as
# "onboarding-agent ... Check that Python is available".
"$PYTHON_BIN" -m pip install --no-cache-dir --no-compile "${PACKAGING_DIR}/../sdk"

echo "==> Step 3/4: prune unnecessary files"
# Windows layout: site-packages at python/Lib/site-packages
# POSIX layout : site-packages at python/lib/python3.13/site-packages
if [[ -d "${PYTHON_DIR}/Lib/site-packages" ]]; then
    SITE_PACKAGES="${PYTHON_DIR}/Lib/site-packages"
else
    SITE_PACKAGES="${PYTHON_DIR}/lib/python3.13/site-packages"
fi
# Tests directories of installed packages - safe to drop, shaves ~20 MB.
find "$SITE_PACKAGES" -type d -name "tests" -prune -exec rm -rf {} + 2>/dev/null || true
# Compiled bytecode - Python will regenerate on first import.
find "$PYTHON_DIR" -type d -name "__pycache__" -prune -exec rm -rf {} + 2>/dev/null || true
find "$PYTHON_DIR" -type f -name "*.pyc" -delete 2>/dev/null || true
# pandas ships internal tests - ~30 MB.
find "${SITE_PACKAGES}/pandas" -type d \( -name "tests" -o -name "_tests" \) -prune -exec rm -rf {} + 2>/dev/null || true

# Tk, and everything that reaches for it. Nothing in this product opens a Tk
# window: the interface is Tauri, the CLI is a terminal. It is dead weight, and
# it is worse than dead weight on Linux, where it stops the AppImage from being
# built at all: linuxdeploy walks the ELF files of the AppDir, reaches
# `_tkinter...so`, and refuses on `Could not find dependency: libtcl9.0.so`.
# The library is right there in python/lib, but resolving a dependency is not
# the same as having deployed the file, and linuxdeploy searches the system
# path, not ours. Removing the extension removes the question.
find "${PYTHON_DIR}" -name "_tkinter*.so" -delete 2>/dev/null || true
find "${PYTHON_DIR}" -type f \( -name "libtcl*" -o -name "libtk*" \) -delete 2>/dev/null || true
find "${PYTHON_DIR}" -maxdepth 3 -type d \
    \( -name "tcl*" -o -name "tk*" -o -name "itcl*" -o -name "thread[0-9]*" \) \
    -prune -exec rm -rf {} + 2>/dev/null || true
find "${PYTHON_DIR}" -maxdepth 4 -type d \( -name "tkinter" -o -name "idlelib" -o -name "turtledemo" \) \
    -prune -exec rm -rf {} + 2>/dev/null || true
find "${PYTHON_DIR}" -maxdepth 4 -type f -name "turtle.py" -delete 2>/dev/null || true

echo "==> Step 4/4: rewrite library paths for bundle-relative resolution"
case "$TARGET" in
    *-apple-darwin|universal-apple-darwin)
        # Rewrite libpython's install_name BEFORE PyO3 links against it so the
        # apollia-desktop and apollia-os binaries automatically embed the right
        # @executable_path reference at link time. Saves a post-build patch pass.
        #
        # Target layout in the final .app:
        #   Contents/MacOS/apollia-desktop       (@executable_path = Contents/MacOS/)
        #   Contents/Resources/apollia-os        (@executable_path = Contents/Resources/)
        #   Contents/Resources/python/lib/libpython3.13.dylib
        #
        # From Contents/MacOS/, the dylib is at ../Resources/python/lib/libpython3.13.dylib
        # From Contents/Resources/, the dylib is at ./python/lib/libpython3.13.dylib
        # We use `@loader_path` for the CLI (relative to where it's loaded from)
        # and `@executable_path/../Resources/python/lib/…` as the canonical id.
        LIBPYTHON="${PYTHON_DIR}/lib/libpython3.13.dylib"
        if [[ -f "$LIBPYTHON" ]]; then
            NEW_ID="@executable_path/../Resources/python/lib/libpython3.13.dylib"
            echo "    macOS: install_name_tool -id '${NEW_ID}' libpython3.13.dylib"
            install_name_tool -id "$NEW_ID" "$LIBPYTHON"
            # Re-sign the dylib ad-hoc because install_name_tool invalidates the
            # existing signature shipped by python-build-standalone.
            codesign --force --sign - "$LIBPYTHON" 2>/dev/null || true
        fi
        ;;
    *-unknown-linux-gnu)
        # python-build-standalone uses $ORIGIN/../lib for the interpreter's RPATH
        # which is already relative. The PyO3-built binaries will have their RPATH
        # set to point into the bundle - handled by patch-prebundle-libpython.sh
        # (Tauri beforeBundleCommand) at bundle time.
        echo "    Linux: using python-build-standalone's default \$ORIGIN RPATH"
        ;;
    *-pc-windows-msvc)
        # Windows: python313.dll sits in python/ (next to python.exe).
        # The apollia-os binary has to find it through:
        #   1. the launcher (apollia-os.bat), which prepends python/ to PATH
        #      before running apollia-os.exe, OR
        #   2. python313.dll copied next to apollia-os.exe at install time (the
        #      zero-config option, done by the CI job at repackaging).
        # No install_name rewrite to do here - Windows uses the standard DLL
        # resolution through PATH and the executable directory.
        echo "    Windows: python.exe + python313.dll resolved via launcher PATH"
        ;;
esac

TOTAL_SIZE=$(du -sh "$PYTHON_DIR" | cut -f1)
echo "==> Python bundle ready at $PYTHON_DIR (${TOTAL_SIZE})"
# Markdownify is no longer in requirements-bundled.txt - a shorter list to check.
"$PYTHON_BIN" -c 'import pandas, openpyxl, pypdf, httpx, bs4; print("  bundled modules import OK")' \
    || echo "warning: some bundled modules failed to import (Windows wheel mismatch?)" >&2
