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
    # A junction left by an earlier version of this function passes the
    # interpreter test from this shell and is still refused by an elevated
    # rustc, so it is removed here and replaced by the copy below. The entry
    # alone is removed, never what it points at.
    if command -v cygpath >/dev/null 2>&1 && [ -L "$dest" ]; then
        cmd //c rmdir "$(cygpath -w "$dest")" >/dev/null 2>&1 || rm -f "$dest" 2>/dev/null || true
    fi
    if [ "$root" != "$dest" ] && ! bundle_python_holds_interpreter "$dest"; then
        # Whatever sits there carries no interpreter: a link left by an earlier
        # layout, or a directory. Remove the entry itself and never its
        # contents, so a junction cannot take the real bundle down with it.
        #
        # Every step here is allowed to fail: the verification below is what
        # decides. Without `|| true`, `set -e` killed the whole recipe on a
        # bare `rmdir` refusal, exit 145 (ERROR_DIR_NOT_EMPTY), printing
        # nothing at all. Measured on 2026-09-08.
        if [ -L "$dest" ]; then
            rm -f "$dest" || true
        elif [ -d "$dest" ]; then
            if command -v cygpath >/dev/null 2>&1; then
                cmd //c rmdir "$(cygpath -w "$dest")" >/dev/null 2>&1 || true
            else
                rmdir "$dest" 2>/dev/null || true
            fi
        fi
        mkdir -p "$(dirname "$dest")" || true
        # Only when the path is free. `ln -sfn` against an existing directory
        # creates the link INSIDE it, which would leave a bundle at
        # <dest>/python that nothing looks for.
        if [ -e "$dest" ] || [ -L "$dest" ]; then
            :
        elif command -v cygpath >/dev/null 2>&1; then
            # A copy, not a junction. A junction is what an ordinary user may
            # create, and this used `mklink //J` for that reason, but Windows
            # refuses to follow a junction from an elevated process when a
            # lower-integrity one created it (error 448, untrusted mount
            # point). The build script of the desktop crate copies a resource
            # through this path, so from an elevated shell every rebuild after
            # a source change died on "file already exists (os error 183)",
            # which names neither the junction nor the elevation. Measured on
            # 2026-09-09. The shell's own test below cannot tell the two
            # apart either: it read the junction fine while rustc could not.
            #
            # A symbolic link needs a privilege the unelevated developer does
            # not have, and git-bash's `ln -s` copies anyway. So the copy is
            # the one form every process on the machine can open. It costs a
            # couple of hundred megabytes once: the next run finds the bundle
            # in place and skips this block entirely.
            cp -R "$root" "$dest" 2>/dev/null || true
        else
            ln -sfn "$root" "$dest" || true
        fi
        # Whatever the link did, a copy always works: no privilege, no
        # junction, no shell convention. It costs a couple of hundred
        # megabytes once, since the next run finds the bundle in place and
        # skips this block entirely. A run that boots is worth more than a
        # link that saves disk.
        if ! bundle_python_holds_interpreter "$dest"; then
            echo "note: linking the Python bundle did not take; copying it to $dest" >&2
            rm -rf "$dest" 2>/dev/null || true
            cp -R "$root" "$dest" 2>/dev/null || true
        fi
    fi
    # Verified on every path, including the one that changed nothing. An
    # earlier version returned success as soon as the destination existed,
    # which let a stale directory through and cost a full run.
    if ! bundle_python_holds_interpreter "$dest"; then
        echo "error: no Python bundle reachable at $dest" >&2
        echo "       The app probes that path at run time; without it every" >&2
        echo "       agent fails to load with \"No module named 'apollia'\"." >&2
        echo "       Bundle found at: $root" >&2
        if [ -e "$dest" ]; then
            echo "       Something else occupies that path. It holds:" >&2
            ls -A "$dest" 2>/dev/null | head -5 | sed 's/^/         /' >&2
            echo "       Remove it and rerun." >&2
        fi
        return 1
    fi
}
