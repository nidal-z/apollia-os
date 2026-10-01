#!/usr/bin/env bash
# Install the MLX inference engine (vllm-mlx) into a Python bundle, in its own
# package directory, apart from the agents' site-packages.
#
# Usage:
#   ./build-mlx-engine.sh <target-triple> <python-dir>
#
# <python-dir> is the `python/` tree build-python-bundle.sh produced. The
# engine lands in <python-dir>/engines/mlx/site-packages, which ships with the
# rest of the tree as `Contents/Resources/python/engines/mlx/` in the .app.
#
# Only aarch64-apple-darwin carries the engine: MLX runs on Apple Silicon and
# nowhere else. Any other triple is refused, so a caller that reaches this
# script for Windows or Linux is a defect, not a silent no-op.
#
# Isolation contract, the one the supervisor relies on: the engine is started
# as
#   PYTHONPATH=<python-dir>/engines/mlx/site-packages \
#     <python-dir>/bin/python3.13 -S -m vllm_mlx.cli serve --host 127.0.0.1 ...
# `-S` keeps the interpreter's own site-packages (the agents' packages) off
# sys.path, and nothing on the agents' side ever adds this directory. The two
# sets carry different versions of the same libraries (numpy, pandas) and
# never meet in one process.
set -euo pipefail

TARGET="${1:?usage: build-mlx-engine.sh <target-triple> <python-dir>}"
PYTHON_DIR="${2:?usage: build-mlx-engine.sh <target-triple> <python-dir>}"

if [[ "$TARGET" != "aarch64-apple-darwin" ]]; then
    echo "error: the MLX engine is for aarch64-apple-darwin only, not '$TARGET'" >&2
    exit 2
fi

PACKAGING_DIR="$(cd "$(dirname "$0")" && pwd)"
REQUIREMENTS="${PACKAGING_DIR}/requirements-mlx-engine.txt"
PYTHON_BIN="${PYTHON_DIR}/bin/python3.13"
ENGINE_DIR="${PYTHON_DIR}/engines/mlx"
SITE_DIR="${ENGINE_DIR}/site-packages"
# Beside the interpreter archive cache of fetch-python-standalone.sh, which
# survives a rebuild of python/: the 300 MB of wheels are downloaded once.
WHEEL_CACHE="$(dirname "$PYTHON_DIR")/.cache/mlx-engine-wheels"

if [[ ! -x "$PYTHON_BIN" ]]; then
    echo "error: no interpreter at $PYTHON_BIN; run build-python-bundle.sh first" >&2
    exit 2
fi

export PYTHONNOUSERSITE=1

# The selection is pinned to the oldest macOS the wheels support, never left
# to the build host: mlx and mlx-metal publish one wheel per macOS major, and
# the one a newer runner would pick does not load on an older machine. The
# sums in requirements-mlx-engine.txt are the sums of exactly this selection.
PLATFORM_ARGS=(
    --only-binary=:all:
    --platform macosx_14_0_arm64
    --python-version 3.13
    --implementation cp
    --abi cp313
)

echo "==> MLX engine: download the hash-locked closure"
mkdir -p "$WHEEL_CACHE"
# --require-hashes: a wheel whose sum differs from the pinned one is refused,
# cached copies included, and so is any dependency the file does not pin.
"$PYTHON_BIN" -m pip download --no-cache-dir --require-hashes \
    "${PLATFORM_ARGS[@]}" -d "$WHEEL_CACHE" -r "$REQUIREMENTS"

echo "==> MLX engine: install into ${SITE_DIR}"
rm -rf "$ENGINE_DIR"
mkdir -p "$SITE_DIR"
"$PYTHON_BIN" -m pip install --no-cache-dir --no-compile --no-index \
    --find-links "$WHEEL_CACHE" --require-hashes \
    "${PLATFORM_ARGS[@]}" --target "$SITE_DIR" -r "$REQUIREMENTS"

# Console scripts carry the absolute path of the build interpreter in their
# shebang, so they are dead once the tree moves into the .app. The engine is
# started with `-m`, as above.
rm -rf "${SITE_DIR}/bin"

# The verdict: the server entry point answers with the bundled interpreter,
# under the isolation contract. A build host that cannot run arm64 code says
# so instead of passing silently.
if [[ "$(uname -s)" == "Darwin" && "$(uname -m)" == "arm64" ]]; then
    PYTHONPATH="$SITE_DIR" "$PYTHON_BIN" -S -m vllm_mlx.cli serve --help >/dev/null
    echo "==> MLX engine: vllm_mlx.cli serve --help answered"
else
    echo "warning: build host is not macOS arm64; the MLX engine was installed but not run" >&2
fi

echo "==> MLX engine ready at ${ENGINE_DIR} ($(du -sh "$ENGINE_DIR" | cut -f1))"
