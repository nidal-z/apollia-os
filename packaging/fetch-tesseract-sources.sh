#!/usr/bin/env bash
# Stage verified Leptonica and Tesseract sources where the build script of the
# `tesseract-rs` crate looks for them, before any `cargo build --features ocr`.
#
# That build script downloads both archives itself, with no checksum, whenever
# its source directory is missing, and it always fetches two language files
# unless they are present. A directory that already exists is taken as is. This
# script puts verified sources there first, so the build has nothing left to
# download and no unverified C++ ends up in a shipped binary.
#
# Usage:
#   fetch-tesseract-sources.sh [--base <dir>]
#     --base <dir> : the directory to fill. Without it, the one the build script
#                    of the pinned `tesseract-rs` computes from the system:
#                      macOS   $HOME/Library/Application Support/tesseract-rs
#                      Linux   $HOME/.tesseract-rs
#                      Windows %APPDATA%\tesseract-rs (Git Bash), or else
#                              %USERPROFILE%\AppData\Roaming\tesseract-rs
#
# What it leaves under the base, and nothing else:
#   third_party/leptonica/   the Leptonica archive, without its top directory
#   third_party/tesseract/   the Tesseract archive, without its top directory
#   tessdata/eng.traineddata and tessdata/tur.traineddata, both empty
#
# The base is cleared first (sources, install trees, library cache, tessdata):
# the build script copies a cached `libleptonica.a` before looking at any
# source, so a library from an earlier build would otherwise survive any change
# of sources.
#
# Every archive is checked against packaging/tesseract-sources-checksums.txt:
# its SHA-256, then the commit GitHub writes in the zip comment, which ties the
# archive to the upstream tag. There is no way to turn either check off.
#
# Env:
#   TESSERACT_SOURCES_DIR : a directory that already holds the archives, named
#                           <project>-<version>.zip (leptonica-1.87.0.zip,
#                           tesseract-5.5.2.zip), for a machine without network.
#                           They are verified exactly like a download.
#
# Exit codes: 0 staged and verified, 1 failure (download, checksum, commit,
# extraction), 2 usage error.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
CHECKSUMS="${SCRIPT_DIR}/tesseract-sources-checksums.txt"

usage() {
    echo "usage: fetch-tesseract-sources.sh [--base <dir>]" >&2
    exit 2
}

BASE=""
while [ "$#" -gt 0 ]; do
    case "$1" in
        --base)
            [ "$#" -ge 2 ] || usage
            BASE="$2"
            shift 2
            ;;
        -h | --help) usage ;;
        *) usage ;;
    esac
done

# The directory `get_custom_out_dir` returns in the build script of the pinned
# tesseract-rs: only HOME or APPDATA move it.
default_base() {
    case "$(uname -s)" in
        Darwin) echo "${HOME:?HOME is not set}/Library/Application Support/tesseract-rs" ;;
        Linux | FreeBSD) echo "${HOME:?HOME is not set}/.tesseract-rs" ;;
        MINGW* | MSYS* | CYGWIN*)
            if [ -n "${APPDATA:-}" ]; then
                echo "$(cygpath -u "$APPDATA")/tesseract-rs"
            elif [ -n "${USERPROFILE:-}" ]; then
                echo "$(cygpath -u "$USERPROFILE")/AppData/Roaming/tesseract-rs"
            else
                echo "==> neither APPDATA nor USERPROFILE is set" >&2
                return 1
            fi
            ;;
        *)
            echo "==> unsupported system: $(uname -s)" >&2
            return 1
            ;;
    esac
}

# SHA256 of a file, with shasum (macOS, perl) or sha256sum (coreutils). Neither
# present is a hard error: the verification is the reason this script exists.
sha256_of() {
    if command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$1" | awk '{print $1}'
    elif command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | awk '{print $1}'
    else
        echo "==> neither shasum nor sha256sum is available; cannot verify" >&2
        echo "    the Tesseract sources. Install one of them." >&2
        return 1
    fi
}

# The comment of a GitHub tag archive: the commit the archive was made from.
zip_comment_of() {
    if command -v unzip >/dev/null 2>&1; then
        unzip -z "$1" | sed '1d' | tr -d '[:space:]'
    elif command -v python3 >/dev/null 2>&1; then
        python3 -I -c 'import sys, zipfile; print(zipfile.ZipFile(sys.argv[1]).comment.decode("ascii", "replace"))' "$1" |
            tr -d '[:space:]'
    else
        echo "==> neither unzip nor python3 is available; cannot read the" >&2
        echo "    commit of $(basename "$1")." >&2
        return 1
    fi
}

extract_zip() {
    if command -v unzip >/dev/null 2>&1; then
        unzip -q "$1" -d "$2"
    elif command -v python3 >/dev/null 2>&1; then
        python3 -I -c 'import sys, zipfile; zipfile.ZipFile(sys.argv[1]).extractall(sys.argv[2])' "$1" "$2"
    else
        echo "==> neither unzip nor python3 is available to extract $(basename "$1")" >&2
        return 1
    fi
}

if [ ! -f "$CHECKSUMS" ]; then
    echo "==> missing ${CHECKSUMS}" >&2
    exit 1
fi

if [ -z "$BASE" ]; then
    BASE="$(default_base)" || exit 1
fi
case "$BASE" in
    "" | "/" | "${HOME:-/}" | "${HOME:-}/")
        echo "==> refusing to clear ${BASE:-an empty path} as the tesseract-rs base" >&2
        exit 1
        ;;
esac

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
mkdir -p "$WORK/archives" "$WORK/stage/third_party"

# One line per archive after the `tesseract-rs <version>` header:
#   <sha256>  <url>  <commit>
NAMES=()
while read -r sum url commit extra <&3; do
    case "$sum" in "" | \#* | tesseract-rs) continue ;; esac
    if [ -z "$url" ] || [ -z "$commit" ] || [ -n "${extra:-}" ]; then
        echo "==> malformed line in $(basename "$CHECKSUMS"): $sum $url $commit" >&2
        exit 1
    fi
    # The build script downloads tag archives from GitHub, nothing else:
    # https://github.com/<owner>/<project>/archive/refs/tags/<version>.zip
    case "$url" in
        https://github.com/*/*/archive/refs/tags/*.zip) ;;
        *)
            echo "==> not a GitHub tag archive: ${url}" >&2
            exit 1
            ;;
    esac
    project="$(echo "$url" | awk -F/ '{print $5}')"
    version="$(basename "$url" .zip)"
    file="${project}-${version}.zip"
    archive="$WORK/archives/$file"

    if [ -n "${TESSERACT_SOURCES_DIR:-}" ]; then
        if [ ! -f "${TESSERACT_SOURCES_DIR}/${file}" ]; then
            echo "==> ${file} not found in ${TESSERACT_SOURCES_DIR}" >&2
            exit 1
        fi
        cp "${TESSERACT_SOURCES_DIR}/${file}" "$archive"
    else
        echo "==> downloading ${url}"
        # HTTP/1.1: over HTTP/2 a slow codeload stream can be cancelled
        # mid-transfer (curl error 92). --retry alone replays only what curl
        # deems transient, never a connection cut mid-body (error 56) nor that
        # cancellation, so --retry-all-errors replays any failure, and each
        # attempt rewrites the archive from its first byte. The checks below
        # read the file only once curl has exited 0, on a complete archive.
        if ! curl --http1.1 -fsSL --retry 5 --retry-all-errors --retry-delay 2 \
            -o "$archive" "$url"; then
            echo "==> download failed: ${url}" >&2
            exit 1
        fi
    fi

    got="$(sha256_of "$archive")" || exit 1
    if [ "$got" != "$sum" ]; then
        echo "==> ${file}: sha256 ${got} differs from the pinned ${sum}" >&2
        exit 1
    fi
    got_commit="$(zip_comment_of "$archive")" || exit 1
    if [ "$got_commit" != "$commit" ]; then
        echo "==> ${file}: commit ${got_commit:-<none>} differs from the pinned ${commit}" >&2
        exit 1
    fi
    echo "==> ${file}: sha256 and commit ${commit} verified"

    # The build script strips the first path component of every entry: the
    # archive must hold exactly one top directory, whose content is the tree.
    mkdir "$WORK/x-$project"
    extract_zip "$archive" "$WORK/x-$project" || exit 1
    tops=("$WORK/x-$project"/*)
    if [ "${#tops[@]}" -ne 1 ] || [ ! -d "${tops[0]}" ]; then
        echo "==> ${file}: expected one top directory, found ${#tops[@]} entries" >&2
        exit 1
    fi
    if [ ! -f "${tops[0]}/CMakeLists.txt" ]; then
        echo "==> ${file}: no CMakeLists.txt under its top directory" >&2
        exit 1
    fi
    mv "${tops[0]}" "$WORK/stage/third_party/$project"
    NAMES+=("$project")
done 3<"$CHECKSUMS"

for want in leptonica tesseract; do
    case " ${NAMES[*]:-} " in
        *" $want "*) ;;
        *)
            echo "==> $(basename "$CHECKSUMS") pins no ${want} archive" >&2
            exit 1
            ;;
    esac
done

# REASON: the build script fetches eng and tur from tessdata_best, unchecked,
# unless both files exist (tesseract-rs 0.4.0 build.rs:492). The product never
# reads them: it hands its own language directory to Tesseract. Two empty files
# of the right name stop that download.
mkdir -p "$WORK/stage/tessdata"
: >"$WORK/stage/tessdata/eng.traineddata"
: >"$WORK/stage/tessdata/tur.traineddata"

mkdir -p "$BASE"
for entry in third_party leptonica tesseract cache tessdata; do
    rm -rf "${BASE:?}/${entry}"
done
mv "$WORK/stage/third_party" "$BASE/third_party"
mv "$WORK/stage/tessdata" "$BASE/tessdata"
echo "==> verified Tesseract and Leptonica sources staged under ${BASE}"
