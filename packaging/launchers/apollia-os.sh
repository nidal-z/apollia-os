#!/usr/bin/env bash
#
# Apollia OS launcher - Linux / macOS.
#
# This script ships in every release archive (linux-* and macos-*).
# It guarantees that apollia-os finds the bundled Python 3.13 interpreter even
# when the user has no Python installed on the system.
#
# Expected layout (extracted from the archive):
#   apollia-os/
#   |-- apollia-os.sh        <- this launcher
#   |-- apollia-os           <- the binary
#   └── python/
#       |-- bin/python3.13   <- bundled interpreter
#       └── lib/...
#
# Usage :
#   ./apollia-os.sh start         # starts the daemon
#   ./apollia-os.sh run <agent>   # any apollia-os command
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"

# 1. PYO3_PYTHON points at the bundled interpreter. The binary, compiled with
#    PyO3 dynamic linking, opens libpython3.13 relative to that executable.
export PYO3_PYTHON="${HERE}/python/bin/python3.13"

if [[ ! -x "$PYO3_PYTHON" ]]; then
    echo "error: bundled Python missing at $PYO3_PYTHON" >&2
    echo "       the archive is incomplete - download it again from" >&2
    echo "       https://github.com/Apollia-OS/apollia-os/releases" >&2
    exit 1
fi

# 2. LD_LIBRARY_PATH (Linux) / DYLD_LIBRARY_PATH (macOS), so the loader
#    trouve libpython3.13.so / .dylib.
case "$(uname -s)" in
    Linux)
        export LD_LIBRARY_PATH="${HERE}/python/lib:${LD_LIBRARY_PATH:-}"
        ;;
    Darwin)
        # macOS: the install_name of the dylib was rewritten at packaging time
        # to @executable_path/../Resources/python/lib/..., which works inside
        # the Tauri bundle; for the standalone CLI it is prepended explicitly.
        export DYLD_LIBRARY_PATH="${HERE}/python/lib:${DYLD_LIBRARY_PATH:-}"
        ;;
esac

# 3. APOLLIA_PYTHON_BUNDLE_DIR: an optional variable the runtime reads to
#    build the per-agent venvs, pointing pip at the bundled interpreter.
export APOLLIA_PYTHON_BUNDLE_DIR="${HERE}/python"

# 4. Run the binary with every argument passed through.
exec "${HERE}/apollia-os" "$@"
