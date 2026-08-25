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
# The asset is chosen on `uname -s` AND `uname -m`: an arm64 host never
# receives an x64 binary. Exit codes render three distinct verdicts:
#   0 : staged and verified
#   1 : failure (download, checksum, broken stage)
#   2 : the pinned upstream release publishes no asset for this
#       OS / arch / backend couple; nothing was attempted
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
# Normalise the machine name: macOS says arm64 where Linux says aarch64.
case "$(uname -m)" in
    arm64 | aarch64) uname_m="arm64" ;;
    x86_64 | amd64)  uname_m="x64" ;;
    *)               uname_m="$(uname -m)" ;;
esac
asset=""
extra_asset=""
case "${uname_s}:${uname_m}:${BACKEND}" in
    Darwin:arm64:*)             asset="llama-${LLAMA_CPP_TAG}-bin-macos-arm64.tar.gz" ;;
    Darwin:x64:*)               asset="llama-${LLAMA_CPP_TAG}-bin-macos-x64.tar.gz" ;;
    Linux:x64:cpu)              asset="llama-${LLAMA_CPP_TAG}-bin-ubuntu-x64.tar.gz" ;;
    Linux:x64:vulkan)           asset="llama-${LLAMA_CPP_TAG}-bin-ubuntu-vulkan-x64.tar.gz" ;;
    Linux:x64:rocm)             asset="llama-${LLAMA_CPP_TAG}-bin-ubuntu-rocm-7.2-x64.tar.gz" ;;
    Linux:arm64:cpu)            asset="llama-${LLAMA_CPP_TAG}-bin-ubuntu-arm64.tar.gz" ;;
    MINGW*:x64:cpu | MSYS*:x64:cpu)   asset="llama-${LLAMA_CPP_TAG}-bin-win-cpu-x64.zip" ;;
    MINGW*:x64:cuda | MSYS*:x64:cuda)
        asset="llama-${LLAMA_CPP_TAG}-bin-win-cuda-12.4-x64.zip"
        # The CUDA build dlopens the CUDA runtime; upstream ships it as a
        # separate archive. Without it the server only starts on machines
        # that installed the CUDA toolkit themselves, which contradicts the
        # bundle's "up-to-date GPU driver only" requirement.
        extra_asset="cudart-llama-bin-win-cuda-12.4-x64.zip"
        ;;
    MINGW*:x64:vulkan | MSYS*:x64:vulkan) asset="llama-${LLAMA_CPP_TAG}-bin-win-vulkan-x64.zip" ;;
    *)
        echo "==> the pinned llama.cpp release (${LLAMA_CPP_TAG}) publishes no" >&2
        echo "    llama-server asset for ${uname_s}/${uname_m}/${BACKEND}." >&2
        echo "    Build llama.cpp from source and pass LLAMA_SERVER_DIR=<bin dir>." >&2
        exit 2
        ;;
esac

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

# Download one release asset into $tmp and verify it against the pinned
# checksum unless explicitly allowed to skip.
fetch_and_verify() {
    local name="$1"
    local url="https://github.com/ggml-org/llama.cpp/releases/download/${LLAMA_CPP_TAG}/${name}"
    echo "==> downloading ${name} (${LLAMA_CPP_TAG})"
    curl -fsSL "$url" -o "${tmp}/${name}"

    if [ "${ALLOW_UNVERIFIED:-}" = "1" ]; then
        echo "==> WARNING: ALLOW_UNVERIFIED=1, skipping checksum verification (dev only)"
    elif [ -f "$CHECKSUMS" ] && grep -q " ${name}\$" "$CHECKSUMS"; then
        local expected actual
        expected="$(grep " ${name}\$" "$CHECKSUMS" | awk '{print $1}')"
        actual="$(sha256_of "${tmp}/${name}")"
        if [ "$expected" != "$actual" ]; then
            echo "==> checksum mismatch for ${name}: expected ${expected}, got ${actual}" >&2
            exit 1
        fi
        echo "==> checksum ok for ${name}"
    else
        echo "==> no pinned checksum for ${name} in ${CHECKSUMS}." >&2
        echo "    Add it (\`shasum -a 256 ${name}\`) or set ALLOW_UNVERIFIED=1 for dev." >&2
        exit 1
    fi
}

fetch_and_verify "$asset"

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

# The companion archive (Windows CUDA runtime) carries DLLs only: stage them
# flat next to the server, where the loader resolves them.
if [ -n "$extra_asset" ]; then
    fetch_and_verify "$extra_asset"
    extra_dir="${tmp}/extra"
    mkdir -p "$extra_dir"
    (cd "$extra_dir" && unzip -q "${tmp}/${extra_asset}")
    find "$extra_dir" -type f -name '*.dll' -exec cp {} "$DEST"/ \;
    echo "==> staged the CUDA runtime DLLs from ${extra_asset}"
fi
