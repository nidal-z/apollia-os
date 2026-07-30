#!/usr/bin/env bash
# Stage the embedded `llama-server` (upstream llama.cpp) into a runners directory.
#
# Local LLM inference runs through this binary (see the `llama_server` module in
# apollia-runtime). It is bundled next to the `apollia-runner-{backend}` STT
# sidecars in the same `runners/` resource dir, where the daemon's
# `locate_llama_server_binary` finds it.
#
# Usage:
#   fetch-llama-server.sh <backend> <dest_runners_dir>
#     backend        : metal | cpu | cuda | vulkan | rocm (matches the GPU build)
#     dest_runners_dir : the staging `runners/` directory
#
# Two sourcing modes:
#   1. LLAMA_SERVER_DIR=/path/to/extracted/llama.cpp/bin  (offline / dev / your
#      own build): copies `llama-server` and its shared libraries from that dir.
#   2. Otherwise: downloads the pinned llama.cpp release asset for the platform
#      and verifies it against packaging/llama-server-checksums.txt (SHA256).
#      Set ALLOW_UNVERIFIED=1 to skip verification (dev only, not for releases).
#
# Env:
#   LLAMA_CPP_TAG   : release tag to fetch (default below); overridable.
#   LLAMA_SERVER_DIR: bundle a local build/extract instead of downloading.
#   ALLOW_UNVERIFIED: "1" to skip checksum verification (dev only).
set -euo pipefail

BACKEND="${1:?usage: fetch-llama-server.sh <backend> <dest_runners_dir>}"
DEST="${2:?usage: fetch-llama-server.sh <backend> <dest_runners_dir>}"

# Pinned upstream llama.cpp release. Bump deliberately (new architectures land
# here); re-fill the checksums file when you do.
LLAMA_CPP_TAG="${LLAMA_CPP_TAG:-b10092}"

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
CHECKSUMS="${SCRIPT_DIR}/llama-server-checksums.txt"

# SHA256 of a file, on both macOS (shasum, from perl) and Linux (sha256sum, from
# coreutils). Neither is universally present: a slim Debian image has no perl,
# some minimal macOS toolchains have no coreutils. Absence of both is a hard
# error, never a skipped verification: that is the whole point of this step.
sha256_of() {
    if command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$1" | awk '{print $1}'
    elif command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | awk '{print $1}'
    else
        echo "==> neither shasum nor sha256sum is available; cannot verify the" >&2
        echo "    llama-server asset. Install one rather than bypassing the check." >&2
        return 1
    fi
}

mkdir -p "$DEST"

# Copy `llama-server` plus every shared library sitting next to it. The official
# builds resolve their libs via @loader_path / $ORIGIN, so keeping them flat
# beside the binary in `runners/` is enough for the loader to find them.
stage_from_dir() {
    local src_dir="$1"
    local bin_ext=""
    case "$(uname -s)" in MINGW* | MSYS* | CYGWIN*) bin_ext=".exe" ;; esac

    local server="${src_dir}/llama-server${bin_ext}"
    if [ ! -f "$server" ]; then
        echo "==> llama-server not found in ${src_dir}" >&2
        return 1
    fi
    cp "$server" "${DEST}/llama-server${bin_ext}"
    chmod +x "${DEST}/llama-server${bin_ext}" || true

    # Shared libraries the server links (libllama, libggml*, ...).
    #
    # Symlinks must be matched and preserved, not skipped. The upstream archives
    # ship each library twice: the versioned real file
    # (`libllama.0.0.10092.dylib`) and a major-version symlink to it
    # (`libllama.0.dylib`), and the binary's load commands name the *symlink*.
    # Copying only regular files therefore stages a set of libraries the loader
    # cannot resolve, and `llama-server` dies at startup with
    # "Library not loaded: @rpath/libllama-common.0.dylib". `cp -a` keeps the
    # links as links on both macOS and GNU coreutils.
    find "$src_dir" -maxdepth 1 \( -type f -o -type l \) \
        \( -name '*.dylib' -o -name '*.so' -o -name '*.so.*' -o -name '*.dll' \) \
        -exec cp -a {} "$DEST"/ \; 2>/dev/null || true

    # macOS: re-sign adhoc (the copy invalidates any signature; Tauri re-signs
    # the whole bundle afterwards). Real files only, signing follows a symlink
    # and would sign the same object twice.
    case "$(uname -s)" in
        Darwin)
            codesign --force --sign - "${DEST}/llama-server" 2>/dev/null || true
            find "$DEST" -maxdepth 1 -type f -name '*.dylib' \
                -exec codesign --force --sign - {} \; 2>/dev/null || true
            ;;
    esac

    # Fail loudly rather than stage an engine that cannot start. A bundle that
    # silently ships a broken llama-server looks like a runtime bug to the user.
    if ! verify_staged_server "${DEST}/llama-server${bin_ext}"; then
        return 1
    fi
    echo "==> staged llama-server (+ libs) into ${DEST}"
}

# Run the staged binary to prove its libraries resolve.
#
# Only meaningful when staging for the host platform; cross-staging (a Linux
# archive prepared on macOS, for instance) cannot execute it, so the check is
# skipped rather than failed.
verify_staged_server() {
    local server="$1"
    if [ ! -x "$server" ]; then
        echo "==> staged llama-server is not executable: ${server}" >&2
        return 1
    fi
    case "${uname_s:-$(uname -s)}:${BACKEND}" in
        Darwin:metal | Darwin:cpu) ;;
        Linux:cpu | Linux:vulkan | Linux:rocm)
            [ "$(uname -s)" = "Linux" ] || return 0
            ;;
        *) return 0 ;;
    esac
    if ! "$server" --version >/dev/null 2>&1; then
        echo "==> staged llama-server cannot start; its libraries do not resolve:" >&2
        "$server" --version 2>&1 | head -5 >&2
        return 1
    fi
    echo "==> verified staged llama-server starts"
}

# ── Mode 1: local dir override ────────────────────────────────────────────────
if [ -n "${LLAMA_SERVER_DIR:-}" ]; then
    echo "==> LLAMA_SERVER_DIR set, bundling from ${LLAMA_SERVER_DIR}"
    stage_from_dir "$LLAMA_SERVER_DIR"
    exit 0
fi

# ── Mode 2: download the pinned release asset ─────────────────────────────────
uname_s="$(uname -s)"
asset=""
case "${uname_s}:${BACKEND}" in
    Darwin:*)                 asset="llama-${LLAMA_CPP_TAG}-bin-macos-arm64.tar.gz" ;;
    Linux:cpu)                asset="llama-${LLAMA_CPP_TAG}-bin-ubuntu-x64.tar.gz" ;;
    Linux:vulkan)             asset="llama-${LLAMA_CPP_TAG}-bin-ubuntu-vulkan-x64.tar.gz" ;;
    Linux:rocm)               asset="llama-${LLAMA_CPP_TAG}-bin-ubuntu-rocm-7.2-x64.tar.gz" ;;
    MINGW*:cpu | MSYS*:cpu)   asset="llama-${LLAMA_CPP_TAG}-bin-win-cpu-x64.zip" ;;
    MINGW*:cuda | MSYS*:cuda) asset="llama-${LLAMA_CPP_TAG}-bin-win-cuda-12.4-x64.zip" ;;
    MINGW*:vulkan | MSYS*:vulkan) asset="llama-${LLAMA_CPP_TAG}-bin-win-vulkan-x64.zip" ;;
    *)
        echo "==> no prebuilt llama-server asset for ${uname_s}/${BACKEND}." >&2
        echo "    Build llama.cpp from source and pass LLAMA_SERVER_DIR=<bin dir>." >&2
        exit 1
        ;;
esac

url="https://github.com/ggml-org/llama.cpp/releases/download/${LLAMA_CPP_TAG}/${asset}"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

echo "==> downloading ${asset} (${LLAMA_CPP_TAG})"
curl -fsSL "$url" -o "${tmp}/${asset}"

# Verify against the pinned checksum unless explicitly allowed to skip.
if [ "${ALLOW_UNVERIFIED:-}" = "1" ]; then
    echo "==> WARNING: ALLOW_UNVERIFIED=1, skipping checksum verification (dev only)"
elif [ -f "$CHECKSUMS" ] && grep -q " ${asset}\$" "$CHECKSUMS"; then
    expected="$(grep " ${asset}\$" "$CHECKSUMS" | awk '{print $1}')"
    actual="$(sha256_of "${tmp}/${asset}")"
    if [ "$expected" != "$actual" ]; then
        echo "==> checksum mismatch for ${asset}: expected ${expected}, got ${actual}" >&2
        exit 1
    fi
    echo "==> checksum ok for ${asset}"
else
    echo "==> no pinned checksum for ${asset} in ${CHECKSUMS}." >&2
    echo "    Add it (\`shasum -a 256 ${asset}\`) or set ALLOW_UNVERIFIED=1 for dev." >&2
    exit 1
fi

# Extract and stage. Release archives place binaries under `build/bin/` or `bin/`.
case "$asset" in
    *.zip) (cd "$tmp" && unzip -q "$asset") ;;
    *.tar.gz) tar -xzf "${tmp}/${asset}" -C "$tmp" ;;
esac
bin_dir="$(dirname "$(find "$tmp" -type f -name 'llama-server*' | head -1)")"
if [ -z "$bin_dir" ] || [ "$bin_dir" = "." ]; then
    echo "==> llama-server not found in the extracted archive" >&2
    exit 1
fi
stage_from_dir "$bin_dir"
