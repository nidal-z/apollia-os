#!/usr/bin/env bash
# Answer a Linux question about this working tree, from a machine that is not
# Linux, in a container. Two questions, one mount:
#
#   compile  cargo clippy --workspace --all-targets --locked -- -D warnings
#   test     cargo test --workspace --no-fail-fast --locked
#
# Why it exists: the repository stopped compiling on Linux while every local
# guard stayed green, because every local guard runs on the platform that
# measures. A type mismatch in crates/apollia-tools/src/tools/rlimits.rs and
# three dead-code enum variants in
# crates/apollia-desktop/src/commands/automation.rs failed four CI jobs and
# skipped three more, the macOS test job among them. Being the platform that
# measures protects from nothing.
#
# Why the test question exists: the compile question links no test binary, so a
# tree can compile on Linux and still fail its suites there. The only run that
# ever asked it was a `docker run` typed by hand, with a mount, two volumes and
# an environment variable that lived in no tracked file, so nobody could replay
# it. It lives here now, and the parallelism it needs is derived inside the
# container instead of being exported by the caller.
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
#   0  the question answered green on the measured target
#   1  it answered red: the tree does not compile, or the suite failed; cargo's
#      output above is the verdict
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
#     bash scripts/linux-check.sh              # x86_64, compiles
#     bash scripts/linux-check.sh arm          # aarch64, compiles
#     bash scripts/linux-check.sh arm test     # aarch64, runs the suites
#     bash scripts/linux-check.sh arm test apollia-tools python_executor
#                                              # aarch64, one crate, one filter
set -euo pipefail

usage() {
    cat >&2 <<'EOF'
usage: bash scripts/linux-check.sh [x86|arm] [compile|test [package [filter]]]

  x86   x86_64-unknown-linux-gnu   (default) the only Linux target whose
        failure stops a release, release.yml:92
  arm   aarch64-unknown-linux-gnu  native on an Apple Silicon host, so faster,
        but both of its release presets carry allow_fail, release.yml:97-98

  compile  (default) the blocking Clippy gate of ci.yml:85
  test     the first step of the Rust Tests job, ci.yml:143-144. Slower, and
           it bounds cargo's parallelism itself; see the perimeter block

  package  test only: one workspace member, passed to cargo as `-p <package>`,
           so a single Linux question costs one crate's build instead of the
           workspace's. A scoped green answers nothing about the other crates,
           and the run says so in its perimeter block
  filter   test only, requires package: a cargo test name filter

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

# The second parameter is the question. Both questions share this file because
# everything that decides the mount is written once here: the platform, the
# daemon and image guards, the triple read out of the container, the throwaway
# copy of gen, the perimeter block, the volumes and the exit-code table. A
# second script would have copied all of it, and copies diverge.
case "${2:-compile}" in
    compile)
        QUESTION="compile"
        LABEL="linux-check"
        CARGO_ARGS="cargo clippy --workspace --all-targets --locked -- -D warnings"
        GREEN="the tree compiles on"
        RED="the tree does NOT compile on"
        ;;
    test)
        QUESTION="test"
        LABEL="linux-test"
        CARGO_ARGS="cargo test --workspace --no-fail-fast --locked"
        GREEN="the suites pass on"
        RED="the suites do NOT pass on"
        ;;
    *)
        echo "ERROR: unknown question '${2}'" >&2
        usage
        exit 2
        ;;
esac

# The optional third and fourth arguments narrow the test question to one
# workspace member and one cargo test filter, so measuring a single crate's
# Linux gap does not cost the whole workspace's build. They are validated
# against the character set of crate and test names before they reach the
# `sh -c` inside the container: an argument this validation refuses would
# otherwise become shell inside the image, and exit 2 keeps "refused" distinct
# from "measured red".
SCOPE=""
if [ "$#" -gt 2 ]; then
    if [ "$QUESTION" != "test" ]; then
        echo "ERROR: a package scope only applies to the test question." >&2
        usage
        exit 2
    fi
    if ! printf '%s' "${3}" | grep -Eq '^[A-Za-z0-9_-]+$'; then
        echo "ERROR: package '${3}' is not a plain crate name, nothing was measured." >&2
        exit 2
    fi
    CARGO_ARGS="cargo test -p ${3} --no-fail-fast --locked"
    SCOPE="-p ${3}"
    if [ "$#" -gt 3 ]; then
        if ! printf '%s' "${4}" | grep -Eq '^[A-Za-z0-9_:]+$'; then
            echo "ERROR: filter '${4}' is not a plain test filter, nothing was measured." >&2
            exit 2
        fi
        CARGO_ARGS="${CARGO_ARGS} ${4}"
        SCOPE="${SCOPE} ${4}"
    fi
    if [ "$#" -gt 4 ]; then
        echo "ERROR: unexpected argument '${5}'" >&2
        usage
        exit 2
    fi
fi

IMAGE="apollia-linux-check:${ARCH}"
DOCKERFILE="scripts/linux-check.Dockerfile"
VOL_CARGO="apollia-linux-check-cargo-${ARCH}"
VOL_TARGET="apollia-linux-check-target-${ARCH}"

if ! command -v docker >/dev/null 2>&1; then
    echo "ERROR: docker is not on PATH, nothing was measured." >&2
    echo "       Install Docker Desktop, or read the verdict of the Clippy job" >&2
    echo "       of .github/workflows/ci.yml on a pushed branch instead." >&2
    exit 2
fi

# The probe is bounded: the daemon can be present and mute, and on this
# machine `docker info` has hung past two minutes with com.docker.backend
# alive. Unbounded, the recipe holds the terminal without a verdict, which
# reads as a measure in progress. Crossing the boundary is exit 2, the same
# "nothing was measured" a missing daemon answers. `timeout` is preferred
# when the host has one; perl ships on every host this script supports.
docker_daemon_answers() {
    if command -v timeout >/dev/null 2>&1; then
        timeout 60 docker info >/dev/null 2>&1
    else
        perl -e 'alarm 60; exec @ARGV' docker info >/dev/null 2>&1
    fi
}

if ! docker_daemon_answers; then
    echo "ERROR: the Docker daemon did not answer within 60 s, nothing was measured." >&2
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
# argument, so the block below reports what ran and not what was asked for. The
# same probe reads the two numbers the parallelism is derived from, for the same
# reason: they belong to the machine that links, not to the host. This host
# reports 28 cores and 256 GiB, the container 24 cores and 7.65 GiB.
PROBE=""
if ! PROBE="$(docker run --rm --platform "$PLATFORM" "$IMAGE" \
    sh -c 'rustc -vV; echo "cores: $(nproc)"; grep ^MemTotal: /proc/meminfo')"; then
    echo "ERROR: could not run ${IMAGE}, nothing was measured." >&2
    exit 2
fi
TRIPLE="$(printf '%s\n' "$PROBE" | awk '/^host: /{print $2}')"
CORES="$(printf '%s\n' "$PROBE" | awk '/^cores: /{print $2}')"
MEM_KB="$(printf '%s\n' "$PROBE" | awk '/^MemTotal:/{print $2}')"

if [ "$TRIPLE" != "$WANT_TRIPLE" ]; then
    echo "ERROR: asked for ${WANT_TRIPLE}, the container reports ${TRIPLE:-nothing}." >&2
    echo "       Nothing was measured: a verdict on the wrong target is worse" >&2
    echo "       than no verdict." >&2
    exit 2
fi

# Linking the test binaries at the container's default parallelism kills the
# linker: 24 jobs for 7.65 GiB ends on `collect2: fatal error: ld terminated
# with signal 9 [Killed]`, measured on this tree. Two jobs carry the whole
# workspace to the end, also measured. One job per 3 GiB, capped by the core
# count, is the rule those two points bound; 3, 4 and 8 were never measured, so
# the divisor is a choice and the block below prints its inputs rather than
# hiding it. The caller sets nothing: that is the property this channel buys.
#
# The compile question is left unbounded on purpose. Clippy links no test
# binary, so it never meets that ceiling, and bounding it would only make the
# channel that already answers 0 slower.
MEM_PER_JOB_KB=3145728
JOBS=""
if [ -n "$CORES" ] && [ -n "$MEM_KB" ]; then
    JOBS=$((MEM_KB / MEM_PER_JOB_KB))
    if [ "$JOBS" -gt "$CORES" ]; then
        JOBS="$CORES"
    fi
    if [ "$JOBS" -lt 1 ]; then
        JOBS=1
    fi
fi

if [ "$QUESTION" = "test" ]; then
    if [ -z "$JOBS" ]; then
        echo "ERROR: the container reported neither its cores nor its memory," >&2
        echo "       so the parallelism could not be derived and nothing was" >&2
        echo "       measured. Running the suites unbounded kills the linker." >&2
        exit 2
    fi
    GIB_PER_JOB=$((MEM_PER_JOB_KB / 1024 / 1024))
    PARALLELISM="CARGO_BUILD_JOBS=${JOBS}, derived in the container from
                    cores=${CORES} and MemTotal=${MEM_KB} kB: one job per
                    ${GIB_PER_JOB} GiB, capped by the core count. The caller
                    sets nothing"
    UNCOVERED_EXTRA="cargo test -p apollia-e2e-tests --features
                    python-tests, the second step of the job this reflects
                    (ci.yml:145-146)"
else
    PARALLELISM="unbounded, cores=${CORES:-unknown} in the container. Clippy
                    links no test binary, so it never meets the ceiling that
                    the test question has to bound itself under"
    UNCOVERED_EXTRA="the suites themselves: this question compiles, it runs
                    nothing"
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
# The schemas left the index (they are build output), so a fresh worktree has
# no gen directory at all: the copy then starts empty and tauri-build
# regenerates the schemas into the writable mount. Two failures were measured
# on such a tree before the two guards below existed: the `cp` killed the whole
# run under `set -e`, and once past that, `docker run` failed with 125 because
# the mountpoint for the writable copy cannot be created inside a read-only
# rootfs. The `mkdir -p` writes an empty, gitignored build-output directory
# into the host tree so the mountpoint exists; it never dirties the tree.
mkdir -p "${TREE}/${GEN_REL}"
cp -a "${TREE}/${GEN_REL}/." "${GEN_COPY}/"

# The member count is measured on the mounted tree, not written by hand, so it
# follows the workspace when a crate is added or removed. A scoped run covers
# one member and says so instead: a green scoped answer must not read as the
# workspace's.
MEMBERS="unknown, cargo metadata failed on the host"
if members="$(cargo metadata --no-deps --format-version 1 2>/dev/null |
    python3 -c 'import json,sys; print(len(json.load(sys.stdin)["packages"]))')"; then
    MEMBERS="${members} workspace members, every target of each"
fi
if [ -n "$SCOPE" ]; then
    MEMBERS="${SCOPE} only; the other workspace members are not measured"
fi

cat <<EOF
==> ${LABEL}
    image           ${IMAGE}  (${DOCKERFILE})
    platform        ${PLATFORM}
    triple          ${TRIPLE}  (rustc -vV, inside the container)
    release preset  ${PRESET}
    source          ${TREE}, mounted read-only
    writable        ${GEN_REL}, a throwaway copy, because tauri-build
                    regenerates the Tauri ACL schemas inside the crate
    command         ${CARGO_ARGS}
    parallelism     ${PARALLELISM}
    covers          ${MEMBERS}
    does not cover  ${OTHER_LINUX}, x86_64-pc-windows-msvc,
                    aarch64-pc-windows-msvc, and the feature presets
                    (cuda, rocm, vulkan, metal): default features only,
                    the same omission as ci.yml:85 and for the same reason
    does not cover  ${UNCOVERED_EXTRA}
    proves nothing  about the runner's environment: its system packages are
                    never confronted with this image's, so a green here is not
                    a promise of a green there
EOF

DOCKER_RUN=(run --rm --platform "$PLATFORM"
    -v "${TREE}:/src:ro"
    -v "${GEN_COPY}:/src/${GEN_REL}"
    -v "${VOL_CARGO}:/usr/local/cargo/registry"
    -v "${VOL_TARGET}:/target"
    -w /src)

rc=0
COUNTS=""
if [ "$QUESTION" = "test" ]; then
    # The log is kept because the counts are extracted from it afterwards, and
    # the exit code is read from PIPESTATUS: `docker run | tee` returns tee's
    # code, which is 0 whatever cargo did.
    LOG="$(mktemp -t apollia-linux-test-log)"
    trap 'rm -rf "$GEN_COPY" "$LOG"' EXIT
    set +e
    docker "${DOCKER_RUN[@]}" -e "CARGO_BUILD_JOBS=${JOBS}" "$IMAGE" \
        sh -c "$CARGO_ARGS" 2>&1 | tee "$LOG"
    rc=${PIPESTATUS[0]}
    set -e
    # The counts come from the instrument this repository already owns, not
    # from a grep: three test functions live in a module named `result`, so
    # `grep -c '^test result:'` counts 81 binaries where the instrument counts
    # 78. The trap is documented at scripts/worktree_verdicts.py:83-86.
    COUNTS="$(python3 - "${TREE}/scripts/worktree_verdicts.py" "$LOG" <<'PYEOF' || true
import importlib.util
import sys
from pathlib import Path

spec = importlib.util.spec_from_file_location("worktree_verdicts", sys.argv[1])
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
log = Path(sys.argv[2]).read_text(encoding="utf-8", errors="replace")
measure = module.measure_cargo_test(module.strip(log), 0)


def value(key: str) -> str:
    read = measure[key]
    return "not measured" if read is None else str(read)


print(f"{value('binaries')} bin, {value('tests')} tst")
PYEOF
)"
    if [ -z "$COUNTS" ]; then
        COUNTS="not measured bin, not measured tst"
    fi
    COUNTS=", ${COUNTS}"
else
    docker "${DOCKER_RUN[@]}" "$IMAGE" sh -c "$CARGO_ARGS" || rc=$?
fi

case "$rc" in
    0)
        echo "==> ${LABEL}: ${GREEN} ${TRIPLE}, exit 0${COUNTS}"
        exit 0
        ;;
    125 | 126 | 127)
        echo "ERROR: docker run failed with ${rc} before cargo ran, nothing was measured." >&2
        exit 2
        ;;
    *)
        echo "==> ${LABEL}: ${RED} ${TRIPLE}, exit ${rc}${COUNTS}" >&2
        exit 1
        ;;
esac
