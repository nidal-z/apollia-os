#!/usr/bin/env bash
# Answer whether this working tree compiles on Linux, from a machine that is
# not Linux, in a container.
#
# Why it exists: the repository stopped compiling on Linux while every local
# guard stayed green, because every local guard runs on the platform that
# measures. A type mismatch in crates/apollia-tools/src/tools/rlimits.rs and
# three dead-code enum variants in
# crates/apollia-desktop/src/commands/automation.rs failed four CI jobs and
# skipped three more, the macOS test job among them. Being the platform that
# measures protects from nothing.
#
# Why clippy and not check: on the very tree this script was written against,
# `cargo check --workspace --all-targets` returned 0 while
# `cargo clippy --workspace --all-targets -- -D warnings` returned 101, on the
# dead-code variants above. A recipe built on `check` would have answered green
# while CI answered red, which is the defect this recipe exists to prevent,
# reproduced inside the tool meant to prevent it.
#
# Exit codes, and the third one is the point:
#
#   0  the tree compiles on the measured target
#   1  the tree does not compile; cargo's output above is the verdict
#   2  no measurement happened: docker missing, daemon down, image not built,
#      unknown argument, or a container that did not run the expected target
#
# 2 is distinct from 1 so "I could not measure" is never read as "the tree is
# fine", and it is non-zero so a caller testing $? fails by default. The daemon
# is deliberately not started for us: that is a host-side side effect with an
# indeterminate wait, and on the machine this was written for the daemon has
# already needed a force quit and a relaunch before it answered.
#
# What it does not cover is printed by the run itself, not hidden here.
#
# Usage:
#     bash scripts/linux-check.sh          # x86_64-unknown-linux-gnu
#     bash scripts/linux-check.sh arm      # aarch64-unknown-linux-gnu
set -euo pipefail

usage() {
    cat >&2 <<'EOF'
usage: bash scripts/linux-check.sh [x86|arm]

  x86   x86_64-unknown-linux-gnu   (default) the only Linux target whose
        failure stops a release, release.yml:92
  arm   aarch64-unknown-linux-gnu  native on an Apple Silicon host, so faster,
        but both of its release presets carry allow_fail, release.yml:97-98

Requires a running Docker daemon. Without one the script exits 2 and measures
nothing, rather than reporting a green tree it never compiled.
EOF
}

TREE="$(git rev-parse --show-toplevel)"
cd "$TREE"

case "${1:-x86}" in
    x86 | x86_64 | amd64)
        PLATFORM="linux/amd64"
        ARCH="x86_64"
        WANT_TRIPLE="x86_64-unknown-linux-gnu"
        PRESET="linux-x86-cpu, blocking                 (release.yml:92)"
        OTHER_LINUX="aarch64-unknown-linux-gnu"
        ;;
    arm | arm64 | aarch64)
        PLATFORM="linux/arm64"
        ARCH="aarch64"
        WANT_TRIPLE="aarch64-unknown-linux-gnu"
        PRESET="linux-arm-cpu and linux-arm-cuda, both allow_fail (release.yml:97-98)"
        OTHER_LINUX="x86_64-unknown-linux-gnu"
        ;;
    *)
        echo "ERROR: unknown architecture '${1}'" >&2
        usage
        exit 2
        ;;
esac

IMAGE="apollia-linux-check:${ARCH}"
DOCKERFILE="scripts/linux-check.Dockerfile"
VOL_CARGO="apollia-linux-check-cargo-${ARCH}"
VOL_TARGET="apollia-linux-check-target-${ARCH}"
CARGO_ARGS="cargo clippy --workspace --all-targets --locked -- -D warnings"

if ! command -v docker >/dev/null 2>&1; then
    echo "ERROR: docker is not on PATH, nothing was measured." >&2
    echo "       Install Docker Desktop, or read the verdict of the Clippy job" >&2
    echo "       of .github/workflows/ci.yml on a pushed branch instead." >&2
    exit 2
fi

if ! docker info >/dev/null 2>&1; then
    echo "ERROR: the Docker daemon does not answer, nothing was measured." >&2
    echo "       Start Docker Desktop, wait until it reports running, then run" >&2
    echo "       this again. It has already needed a force quit and a relaunch" >&2
    echo "       on this machine before it answered." >&2
    exit 2
fi

if ! docker image inspect "$IMAGE" >/dev/null 2>&1; then
    echo "==> building ${IMAGE}, first run for this architecture, a few minutes"
    if ! docker build --platform "$PLATFORM" -t "$IMAGE" - <"$DOCKERFILE"; then
        echo "ERROR: could not build ${IMAGE}, nothing was measured." >&2
        exit 2
    fi
fi

# The triple is read out of the container rather than deduced from the platform
# argument, so the block below reports what ran and not what was asked for.
TRIPLE=""
if ! TRIPLE="$(docker run --rm --platform "$PLATFORM" "$IMAGE" rustc -vV |
    awk '/^host: /{print $2}')"; then
    echo "ERROR: could not run ${IMAGE}, nothing was measured." >&2
    exit 2
fi

if [ "$TRIPLE" != "$WANT_TRIPLE" ]; then
    echo "ERROR: asked for ${WANT_TRIPLE}, the container reports ${TRIPLE:-nothing}." >&2
    echo "       Nothing was measured: a verdict on the wrong target is worse" >&2
    echo "       than no verdict." >&2
    exit 2
fi

# One directory of the tree cannot be read-only, and it was found by running
# rather than by reading: tauri-build regenerates the Tauri ACL schemas inside
# the crate itself, crates/apollia-desktop/gen/schemas/*.json, all four tracked
# by git. A fully read-only source fails its build script with "Read-only file
# system (os error 30)", and only after apollia-tools compiles, which is why the
# red direction of this guard never reached it.
#
# The tree stays read-only and that one directory is replaced by a throwaway
# copy. A Linux run writes its linux-schema.json into the copy, so it never
# appears in the measured tree, where it would be an untracked file that the
# host platform does not produce.
GEN_REL="crates/apollia-desktop/gen"
GEN_COPY="$(mktemp -d -t apollia-linux-check)"
trap 'rm -rf "$GEN_COPY"' EXIT
cp -a "${TREE}/${GEN_REL}/." "${GEN_COPY}/"

# The member count is measured on the mounted tree, not written by hand, so it
# follows the workspace when a crate is added or removed.
MEMBERS="unknown, cargo metadata failed on the host"
if members="$(cargo metadata --no-deps --format-version 1 2>/dev/null |
    python3 -c 'import json,sys; print(len(json.load(sys.stdin)["packages"]))')"; then
    MEMBERS="${members} workspace members, every target of each"
fi

cat <<EOF
==> linux-check
    image           ${IMAGE}  (${DOCKERFILE})
    platform        ${PLATFORM}
    triple          ${TRIPLE}  (rustc -vV, inside the container)
    release preset  ${PRESET}
    source          ${TREE}, mounted read-only
    writable        ${GEN_REL}, a throwaway copy, because tauri-build
                    regenerates the Tauri ACL schemas inside the crate
    command         ${CARGO_ARGS}
    covers          ${MEMBERS}
    does not cover  ${OTHER_LINUX}, x86_64-pc-windows-msvc,
                    aarch64-pc-windows-msvc, and the feature presets
                    (cuda, rocm, vulkan, metal): default features only,
                    the same omission as ci.yml:85 and for the same reason
EOF

rc=0
docker run --rm --platform "$PLATFORM" \
    -v "${TREE}:/src:ro" \
    -v "${GEN_COPY}:/src/${GEN_REL}" \
    -v "${VOL_CARGO}:/usr/local/cargo/registry" \
    -v "${VOL_TARGET}:/target" \
    -w /src \
    "$IMAGE" \
    sh -c "$CARGO_ARGS" || rc=$?

case "$rc" in
    0)
        echo "==> linux-check: the tree compiles on ${TRIPLE}"
        exit 0
        ;;
    125 | 126 | 127)
        echo "ERROR: docker run failed with ${rc} before cargo ran, nothing was measured." >&2
        exit 2
        ;;
    *)
        echo "==> linux-check: the tree does NOT compile on ${TRIPLE}, cargo exit ${rc}" >&2
        exit 1
        ;;
esac
