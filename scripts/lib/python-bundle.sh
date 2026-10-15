# shellcheck shell=bash
# Link a dev build against the bundled interpreter. Sourced by the justfile.
#
# A dev build that links whatever libpython the host resolves, while the
# application points PYTHONHOME at the bundle under target/, runs two different
# CPython builds: the interpreter cannot find its own standard library and
# every agent fails at boot with `ModuleNotFoundError: No module named
# '_opcode'`. The application still starts, so the failure reads as a missing
# agent rather than as a broken build. Every recipe that compiles the desktop
# or the CLI therefore goes through `python_bundle_activate` first.

# shellcheck source=../bundle_python_env.sh
source "$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/bundle_python_env.sh"

# Echo the bundle root of the tree in $PWD and lay down the macOS dev link.
# Returns 1, naming the remedy, when the tree holds no bundle: a caller that
# carried on would compile for minutes and then measure agents that cannot
# load.
python_bundle_resolve() {
    local root
    # Any triple and either layout: target/python-bundle/<triple>/python as the
    # release stages it, or target/debug/python as a plain build leaves it.
    root="$(bundle_python_find || true)"
    if [ -z "$root" ]; then
        echo "error: no Python bundle in target/python-bundle or target/debug." >&2
        echo "       Every agent would fail to load at boot. Build one first:" >&2
        echo "         bash packaging/build-python-bundle.sh <triple> target/python-bundle/<triple>" >&2
        echo "       Triples: x86_64-apple-darwin, aarch64-apple-darwin," >&2
        echo "                x86_64-unknown-linux-gnu, x86_64-pc-windows-msvc" >&2
        return 1
    fi
    # The linked install name is @executable_path/../Resources/python/lib, the
    # packaged layout. In a dev run @executable_path is target/debug, so dyld
    # looks in target/Resources and the application dies before main(). A
    # symlink makes the dev tree answer the same path the bundle does.
    # DYLD_FALLBACK_LIBRARY_PATH would be too late: the interpreter is set up
    # inside the process, after dyld has resolved. macOS alone: elsewhere the
    # link answers nothing, and under git-bash `ln -s` is emulated and can fail.
    if [ "$(uname -s)" = "Darwin" ]; then
        mkdir -p target/Resources
        ln -sfn "$root" target/Resources/python
    fi
    printf '%s\n' "$root"
}

# Export PYO3_PYTHON and RUSTFLAGS so cargo links against the bundle, and make
# the bundle reachable from target/debug. Returns 1 when there is no bundle.
# Bring the SDK a bundle carries up to the one in this tree.
#
# A bundle is built once and reused for weeks, and its `apollia` package is
# the one `pip` copied that day. The packages the application ships are written
# against the SDK of the tree: with a bundle built before an SDK change, each
# of them fails to load on its first import ("cannot import name ... from
# 'apollia'"). The SDK is pure Python with no dependency, so the folder is
# copied over; nothing is downloaded. A bundle already up to date is left
# alone.
python_bundle_sync_sdk() {
    local root="$1" site
    for site in "$root/lib/python3.13/site-packages" "$root/Lib/site-packages"; do
        [ -d "$site" ] || continue
        if diff -rq -x __pycache__ "$PWD/sdk/apollia" "$site/apollia" >/dev/null 2>&1; then
            continue
        fi
        rm -rf "$site/apollia" || return 1
        cp -R "$PWD/sdk/apollia" "$site/apollia" || return 1
        find "$site/apollia" -name __pycache__ -type d -prune -exec rm -rf {} + 2>/dev/null || true
        echo "python bundle: SDK refreshed from sdk/apollia in $site" >&2
    done
}

# Give a bundle of an Apple Silicon Mac the MLX engine it lacks.
#
# The engine is installed when the bundle is built, and a bundle is reused for
# weeks: one built before that step existed, or with APOLLIA_SKIP_MLX_ENGINE=1,
# has no engines/mlx. The application then starts, lists no MLX engine, and
# answers every MLX model with "the MLX engine is not installed on this
# machine", which reads as a product fault. The engine is installed here, once
# (its wheels are cached beside the bundle). A failure, a machine off line for
# instance, is said and does not stop the recipe: everything but MLX still
# runs. APOLLIA_SKIP_MLX_ENGINE=1 leaves the bundle as it is.
python_bundle_mlx_engine() {
    local root="$1"
    [ "$(uname -s)" = "Darwin" ] && [ "$(uname -m)" = "arm64" ] || return 0
    [ "${APOLLIA_SKIP_MLX_ENGINE:-}" = "1" ] && return 0
    [ -d "$root/engines/mlx/site-packages" ] && return 0
    echo "python bundle: no MLX engine in $root, installing it (once)" >&2
    if ! bash "$PWD/packaging/build-mlx-engine.sh" aarch64-apple-darwin "$root" >&2; then
        echo "warning: the MLX engine could not be installed; MLX models will not load." >&2
        echo "         Retry with: bash packaging/build-mlx-engine.sh aarch64-apple-darwin $root" >&2
    fi
    return 0
}

python_bundle_activate() {
    local root
    root="$(python_bundle_resolve)" || return 1
    python_bundle_mlx_engine "$root"
    bundle_python_env "$root" || return 1
    bundle_python_link_dev "$root" "$PWD/target/debug/python" || return 1
    python_bundle_sync_sdk "$root" || return 1
    if [ -d "$PWD/target/debug/python" ] && [ ! "$root" -ef "$PWD/target/debug/python" ]; then
        python_bundle_sync_sdk "$PWD/target/debug/python" || return 1
    fi
    # The desktop declares crates/apollia-desktop/python as a resource, and a
    # dev build copies it over target/debug/python, in place: a bundle a
    # release build staged there brings its old SDK back, and macOS kills a
    # binary rewritten in place at its next start (the MLX engine died on
    # SIGKILL). The staging belongs to the release build, which redoes it; a
    # dev build moves it aside, under target/, and says where.
    local stage="$PWD/crates/apollia-desktop/python" aside entry
    if [ -d "$stage/lib" ]; then
        aside="$PWD/target/desktop-python-stage-$(date +%Y%m%d-%H%M%S)"
        mkdir -p "$aside" || return 1
        for entry in "$stage"/* "$stage"/.[!.]*; do
            [ -e "$entry" ] || continue
            [ "$(basename "$entry")" = ".gitkeep" ] && continue
            mv "$entry" "$aside/" || return 1
        done
        echo "python bundle: the bundle staged for a release build was moved to $aside" >&2
    fi
    export PYO3_PYTHON="$BUNDLE_PY"
    # The bundle is relocatable but its sysconfig still names its build path,
    # so PyO3 emits `-L /install/lib` and the link fails on "library
    # 'python3.13' not found". The release workflow passes the same flag.
    export RUSTFLAGS="${RUSTFLAGS:-} -L $BUNDLE_LIBDIR"
    # -L tells the linker where libpython is, not the loader. On Linux an
    # RPATH makes the built binary carry the answer; without it only a
    # launcher that exports LD_LIBRARY_PATH can start it. macOS needs none:
    # the install name holds it.
    if [ "$(uname -s)" = "Linux" ]; then
        export RUSTFLAGS="$RUSTFLAGS -C link-arg=-Wl,-rpath,$BUNDLE_LIBDIR"
    fi
}
