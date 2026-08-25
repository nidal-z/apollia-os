#!/usr/bin/env bash
# Fetch a python-build-standalone distribution for a given Rust target triple.
#
# Usage:
#   ./fetch-python-standalone.sh <target-triple> <output-dir>
#
# Example:
#   ./fetch-python-standalone.sh aarch64-apple-darwin ./build/python
#
# Exits 0 on success, leaves the extracted `python/` tree in <output-dir>/python/.
#
# Pinned version (update deliberately, not automatically):
#   - cpython: 3.13.13
#   - pbs release tag: 20260414
#
# Sources: https://github.com/astral-sh/python-build-standalone/releases
set -euo pipefail

TARGET="${1:?usage: fetch-python-standalone.sh <target-triple> <output-dir>}"
OUT_DIR="${2:?usage: fetch-python-standalone.sh <target-triple> <output-dir>}"

PBS_TAG="20260414"
CPYTHON_VERSION="3.13.13"

# Map Rust target triple → python-build-standalone archive name.
# `install_only` variant is the slimmed-down tarball (no debug symbols, no tests).
case "$TARGET" in
    aarch64-apple-darwin)
        PBS_TRIPLE="aarch64-apple-darwin"
        ;;
    x86_64-apple-darwin)
        PBS_TRIPLE="x86_64-apple-darwin"
        ;;
    x86_64-unknown-linux-gnu)
        PBS_TRIPLE="x86_64-unknown-linux-gnu"
        ;;
    aarch64-unknown-linux-gnu)
        PBS_TRIPLE="aarch64-unknown-linux-gnu"
        ;;
    x86_64-pc-windows-msvc)
        # Depuis PBS 2025+, le suffix `-shared` a été dropped pour les
        # archives install_only (cf. probe URLs sur release 20260414).
        # Format réel : cpython-X.Y.Z+TAG-x86_64-pc-windows-msvc-install_only.tar.gz
        PBS_TRIPLE="x86_64-pc-windows-msvc"
        ;;
    aarch64-pc-windows-msvc)
        # PBS ships aarch64 Windows depuis 20240909+.
        # Si la release pinned ne l'a pas, le téléchargement échouera bruyamment.
        PBS_TRIPLE="aarch64-pc-windows-msvc"
        ;;
    universal-apple-darwin)
        echo "==> universal-apple-darwin: fetch both architectures and lipo-merge"
        # Recurse to fetch both arches, then merge dylibs+exes via lipo.
        exec "$(dirname "$0")/build-universal-python.sh" "$OUT_DIR"
        ;;
    *)
        echo "error: unsupported target triple '$TARGET'" >&2
        exit 2
        ;;
esac

# Windows distributions sont packagées en .tar.gz comme les autres OS (PBS
# n'utilise pas .zip pour `install_only`). L'archive contient `python/` au
# top-level, identique à Linux/macOS.

ARCHIVE="cpython-${CPYTHON_VERSION}+${PBS_TAG}-${PBS_TRIPLE}-install_only.tar.gz"
URL="https://github.com/astral-sh/python-build-standalone/releases/download/${PBS_TAG}/${ARCHIVE}"

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
CHECKSUMS="${SCRIPT_DIR}/python-standalone-checksums.txt"

# SHA256 of a file, on both macOS (shasum, from perl) and Linux (sha256sum,
# from coreutils). Absence of both is a hard error, never a skipped
# verification: that is the whole point of this step.
sha256_of() {
    if command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$1" | awk '{print $1}'
    elif command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | awk '{print $1}'
    else
        echo "==> neither shasum nor sha256sum is available; cannot verify the" >&2
        echo "    python-build-standalone archive. Install one rather than bypassing." >&2
        return 1
    fi
}

mkdir -p "$OUT_DIR"
CACHE_DIR="${OUT_DIR}/.cache"
mkdir -p "$CACHE_DIR"

CACHED_ARCHIVE="${CACHE_DIR}/${ARCHIVE}"
if [[ -f "$CACHED_ARCHIVE" ]]; then
    echo "==> Using cached archive: $CACHED_ARCHIVE"
else
    echo "==> Downloading $URL"
    curl --fail --location --output "$CACHED_ARCHIVE" "$URL"
fi

# Verify against the pinned checksum before extracting anything, cached copies
# included: the cache directory is plain files anyone can rewrite.
if [[ ! -f "$CHECKSUMS" ]] || ! grep -q " ${ARCHIVE}\$" "$CHECKSUMS"; then
    echo "==> no pinned checksum for ${ARCHIVE} in ${CHECKSUMS}." >&2
    echo "    Add it (\`shasum -a 256 ${ARCHIVE}\`); see the header of that file." >&2
    exit 1
fi
EXPECTED="$(grep " ${ARCHIVE}\$" "$CHECKSUMS" | awk '{print $1}')"
ACTUAL="$(sha256_of "$CACHED_ARCHIVE")"
if [[ "$EXPECTED" != "$ACTUAL" ]]; then
    echo "==> checksum mismatch for ${ARCHIVE}: expected ${EXPECTED}, got ${ACTUAL}" >&2
    rm -f "$CACHED_ARCHIVE"
    exit 1
fi
echo "==> checksum ok for ${ARCHIVE}"

PYTHON_DIR="${OUT_DIR}/python"
# python-build-standalone ships some macOS files with the user-immutable (uchg)
# flag, which makes `rm -rf` fail with "Directory not empty" on a rebuild.
# Clear the flag on the existing tree before removing it.
if [[ "$(uname)" == "Darwin" && -d "$PYTHON_DIR" ]]; then
    chflags -R nouchg "$PYTHON_DIR" 2>/dev/null || true
fi
rm -rf "$PYTHON_DIR"
echo "==> Extracting to $PYTHON_DIR"
tar -xzf "$CACHED_ARCHIVE" -C "$OUT_DIR"
# The tarball extracts as `python/` - already the layout we want.

# Strip symbols on Linux to shave ~15 MB; macOS `install_only` is already stripped.
if [[ "$TARGET" == *"linux"* ]]; then
    find "$PYTHON_DIR" -type f \( -name "*.so" -o -name "python3.13" \) \
        -exec strip --strip-unneeded {} \; 2>/dev/null || true
fi

echo "==> Python bundle ready at $PYTHON_DIR"
# Layout diffère entre POSIX (bin/python3.13) et Windows (python.exe directement
# à la racine, plus Lib/ et DLLs/).
if [[ -x "${PYTHON_DIR}/bin/python3.13" ]]; then
    "${PYTHON_DIR}/bin/python3.13" --version
elif [[ -f "${PYTHON_DIR}/python.exe" ]]; then
    "${PYTHON_DIR}/python.exe" --version
else
    echo "warning: could not locate python interpreter in $PYTHON_DIR" >&2
fi
