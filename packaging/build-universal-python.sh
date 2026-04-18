#!/usr/bin/env bash
# Build a macOS universal2 Python distribution by lipo-merging arm64 and x86_64.
#
# python-build-standalone does not ship a universal2 `install_only` variant, so
# we assemble it ourselves: fetch both single-arch bundles and `lipo -create`
# all Mach-O binaries in-place.
#
# Usage:
#   ./build-universal-python.sh <output-dir>
#
# Produces <output-dir>/python/ with universal2 binaries.
set -euo pipefail

OUT_DIR="${1:?usage: build-universal-python.sh <output-dir>}"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

echo "==> Fetching aarch64-apple-darwin bundle"
"${SCRIPT_DIR}/fetch-python-standalone.sh" aarch64-apple-darwin "${TMP_DIR}/arm64"

echo "==> Fetching x86_64-apple-darwin bundle"
"${SCRIPT_DIR}/fetch-python-standalone.sh" x86_64-apple-darwin "${TMP_DIR}/x86_64"

PYTHON_DIR="${OUT_DIR}/python"
rm -rf "$PYTHON_DIR"
# Start from the arm64 tree (full file structure), then lipo-merge binaries.
cp -R "${TMP_DIR}/arm64/python" "$PYTHON_DIR"

# lipo-merge every Mach-O binary (executables + .dylib + .so extensions).
#
# `file` reports "Mach-O" for both executables and dynamic libraries. We skip
# anything else (Python source, data, archives) — those are architecture-agnostic.
echo "==> Lipo-merging Mach-O binaries into universal2"
count=0
while IFS= read -r arm_file; do
    rel_path="${arm_file#${PYTHON_DIR}/}"
    x86_file="${TMP_DIR}/x86_64/python/${rel_path}"
    if [[ ! -f "$x86_file" ]]; then
        continue
    fi
    if ! file "$arm_file" | grep -q "Mach-O"; then
        continue
    fi
    lipo -create "$arm_file" "$x86_file" -output "$arm_file" 2>/dev/null || {
        echo "  warn: lipo failed on $rel_path — keeping arm64 only" >&2
        continue
    }
    count=$((count + 1))
done < <(find "$PYTHON_DIR" -type f)
echo "==> Merged $count binaries into universal2"

echo "==> Universal2 Python bundle ready at $PYTHON_DIR"
file "${PYTHON_DIR}/bin/python3.13"
