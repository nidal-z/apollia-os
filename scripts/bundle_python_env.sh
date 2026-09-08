# shellcheck shell=bash
# Resolve a Python bundle's interpreter and import library, whatever the shape.
#
# python-build-standalone lays a bundle out differently per system: Unix keeps
# `bin/python3.13` beside `lib/`, Windows keeps `python.exe` at the root beside
# `libs/`. Every dev recipe named the Unix shape alone, so on Windows a bundle
# that was built, complete and correct was reported absent, the app fell back
# to the developer's own interpreter, and every agent failed to load with
# "No module named 'apollia'". Measured on 2026-09-08, on a machine where
# build-python-bundle.sh had just finished successfully.
#
# The paths are handed to native tools (rustc, PyO3), so they come out in the
# form those tools can open: git-bash's `/c/Users/...` is its own POSIX view
# and rustc opens nothing under that name.
#
# Sourced by the justfile. Sets BUNDLE_PY and BUNDLE_LIBDIR; returns 1 when the
# directory holds neither shape.

bundle_python_native() {
    if command -v cygpath >/dev/null 2>&1; then
        cygpath -m "$1"
    else
        printf '%s' "$1"
    fi
}

# True when a directory holds an interpreter, either layout.
bundle_python_holds_interpreter() {
    [ -x "$1/python.exe" ] || [ -x "$1/bin/python3.13" ]
}

bundle_python_env() {
    local root="$1"
    if [ -x "$root/python.exe" ]; then
        BUNDLE_PY="$(bundle_python_native "$root/python.exe")"
        BUNDLE_LIBDIR="$(bundle_python_native "$root/libs")"
    elif [ -x "$root/bin/python3.13" ]; then
        BUNDLE_PY="$(bundle_python_native "$root/bin/python3.13")"
        BUNDLE_LIBDIR="$(bundle_python_native "$root/lib")"
    else
        return 1
    fi
}

# The first bundle among the per-triple layout release.yml stages and the one a
# plain build leaves in target/debug. Echoes the root; returns 1 when none holds
# an interpreter.
bundle_python_find() {
    local candidate
    for candidate in "$PWD"/target/python-bundle/*/python "$PWD/target/debug/python"; do
        # An unmatched glob stays literal under bash, so the directory test is
        # what rejects it rather than a nonexistent path reaching the caller.
        [ -d "$candidate" ] || continue
        if bundle_python_env "$candidate" 2>/dev/null; then
            printf '%s' "$candidate"
            return 0
        fi
    done
    return 1
}

# Make the bundle reachable from the executable's own directory.
#
# At run time the app probes `<exe dir>/python` (and, on macOS,
# `<exe dir>/../Resources/python`), which is the packaged layout. A dev build
# lives in target/debug while the bundle sits under target/python-bundle/<triple>,
# so without a link the app finds nothing, falls back to the developer's own
# interpreter and every agent fails to load with "No module named 'apollia'".
# The macOS half of this was already handled by the recipes; Windows and Linux
# were not. Measured on 2026-09-08, on a Windows machine with a complete bundle.
#
# Idempotent, and it never replaces a real directory: a bundle built straight
# into the destination is left exactly where it is.
bundle_python_link_dev() {
    local root="$1" dest="$2"
    [ -n "$root" ] || return 0
    if [ "$root" != "$dest" ] && ! bundle_python_holds_interpreter "$dest"; then
        # Whatever sits there carries no interpreter: a link left by an earlier
        # layout, or an empty directory. Remove the entry itself and never its
        # contents, so a junction cannot take the real bundle down with it.
        if [ -L "$dest" ]; then
            rm -f "$dest"
        elif [ -d "$dest" ]; then
            if command -v cygpath >/dev/null 2>&1; then
                cmd //c rmdir "$(cygpath -w "$dest")" >/dev/null 2>&1
            else
                rmdir "$dest" 2>/dev/null
            fi
        fi
        mkdir -p "$(dirname "$dest")"
        if command -v cygpath >/dev/null 2>&1; then
            # An NTFS junction, which an ordinary user may create; a symbolic
            # link needs a privilege, and git-bash's emulated `ln -s` would
            # copy the 200 MB of the bundle on every run instead of pointing
            # at it.
            cmd //c mklink /J "$(cygpath -w "$dest")" "$(cygpath -w "$root")" >/dev/null 2>&1
        else
            ln -sfn "$root" "$dest"
        fi
    fi
    # Verified on every path, including the one that changed nothing. The first
    # version of this returned early when the destination already existed, so a
    # stale directory sitting there was reported as success and the app booted
    # on the system interpreter anyway. Measured on 2026-09-08: that early
    # return cost a full run.
    if ! bundle_python_holds_interpreter "$dest"; then
        echo "error: no Python bundle reachable at $dest" >&2
        echo "       The app probes that path at run time; without it every" >&2
        echo "       agent fails to load with \"No module named 'apollia'\"." >&2
        echo "       Bundle found at: $root" >&2
        echo "       If something else occupies that path, remove it and rerun." >&2
        return 1
    fi
}
