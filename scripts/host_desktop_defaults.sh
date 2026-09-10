#!/usr/bin/env bash
# Host defaults for the desktop build recipes.
#
# The recipes used to hardcode the Apple Silicon triple, the Metal STT runner
# and, through it, the Metal engine. Run anywhere else, `just desktop-build`
# downloaded a macOS Python bundle and died extracting it: on Windows tar
# refused the symlinks the bundle carries, on Linux the bundled `python3.13`
# answered "cannot execute binary file". Both measured on 2026-09-08, on the
# same machine, one in PowerShell and one under WSL.
#
# Source it, then call the three functions. Each answers for the host it runs
# on and each is overridable by the caller, so an explicit argument still wins.

# The triple rustc itself reports. Authoritative, and it avoids maintaining a
# uname-to-triple table that would be wrong on exactly the platform nobody
# tested. Falls back to uname only when rustc is absent, which the build would
# fail on two commands later anyway.
host_desktop_triple() {
    local triple
    if triple="$(rustc -vV 2>/dev/null | sed -n 's/^host: //p')" && [ -n "$triple" ]; then
        printf '%s\n' "$triple"
        return 0
    fi
    case "$(uname -s)" in
        Darwin) case "$(uname -m)" in
                    arm64 | aarch64) printf 'aarch64-apple-darwin\n' ;;
                    *) printf 'x86_64-apple-darwin\n' ;;
                esac ;;
        Linux) case "$(uname -m)" in
                   aarch64 | arm64) printf 'aarch64-unknown-linux-gnu\n' ;;
                   *) printf 'x86_64-unknown-linux-gnu\n' ;;
               esac ;;
        MINGW* | MSYS* | CYGWIN*) printf 'x86_64-pc-windows-msvc\n' ;;
        *) return 1 ;;
    esac
}

# The STT runners to stage. Metal exists on Darwin only; everywhere else the
# release matrix stages `cpu` and nothing more, so this matches what CI ships.
host_desktop_runners() {
    case "${1:-$(host_desktop_triple)}" in
        *-apple-darwin) printf 'cpu metal\n' ;;
        # Windows ships the Vulkan runner next to the processor one: the
        # runtime recommends Vulkan for every AMD and Intel card and degrades
        # to the processor only when that binary is absent, which it always
        # was. Building it needs the Vulkan SDK (`VULKAN_SDK`), the same
        # requirement release.yml already installs for its windows-x86-vulkan
        # preset.
        *-pc-windows-msvc) printf 'cpu vulkan\n' ;;
        *) printf 'cpu\n' ;;
    esac
}

# The llama-server build to fetch. Left empty on Darwin so bundle-cli.sh keeps
# deriving Metal from the runner list, which is the path every existing caller
# already exercises. Elsewhere it is the GPU backend the release matrix ships,
# and naming it explicitly is what stops `cpu` from being inferred.
host_desktop_llama_backend() {
    case "${1:-$(host_desktop_triple)}" in
        *-apple-darwin) printf '\n' ;;
        *) printf 'vulkan\n' ;;
    esac
}

# The bundle configuration patch to pass to `cargo tauri build`, or nothing.
#
# tauri.conf.json carries an updater public key and `createUpdaterArtifacts`,
# which together make the build demand TAURI_SIGNING_PRIVATE_KEY. Without it
# the build runs to completion, writes its installers, and only then refuses,
# so the operator reads a failure over artifacts that are sitting on disk and
# are perfectly usable. Measured on 2026-09-08, on a Linux build that had just
# produced its .deb and its AppImage.
#
# The release workflow already answers this by building with the artifacts
# turned off and saying so. This is the same answer, so that a local build and
# a CI build fail and succeed for the same reasons.
desktop_updater_patch() {
    if [ -n "${TAURI_SIGNING_PRIVATE_KEY:-}" ]; then
        return 0
    fi
    echo "==> TAURI_SIGNING_PRIVATE_KEY is not set: building without updater" >&2
    echo "    artifacts. The installers are complete; what this build does not" >&2
    echo "    produce is the signature the auto-update channel reads." >&2
    printf '{"bundle":{"createUpdaterArtifacts":false}}\n'
}

# The compiler flags the STT runner's C++ (whisper.cpp, through whisper-rs-sys)
# is built with on Windows.
#
# The `cmake` crate that drives that build sets `CMAKE_C_FLAGS_<BUILD_TYPE>`
# to the `cc` crate's own flags, and it computes those at optimisation level
# zero, whatever cargo's profile. On MSVC that variable is the one carrying
# `/O2`, so every whisper.cpp built this way ran unoptimised: the crate's own
# source calls the override "bad". Measured on 2026-09-10 on a Ryzen 9 5950X,
# a 5.5 s file through the release runner: 311 s at whisper's default four
# threads, 97.7 s at sixteen threads, and 8.9 s at sixteen threads once these
# flags were in force, which is the debug runner's own figure (9.2 s). The
# override is skipped for a variable the caller defined, and whisper-rs-sys
# passes every `CMAKE_*` variable from its environment through, which is the
# seam used here. `-MT` keeps the static runtime the desktop bundle links
# against. Dashes rather than slashes, which cl accepts alike: under git-bash
# an argument starting with a single slash is rewritten into a Windows path,
# and `/MT` reached the compiler as `C:/Program Files/Git/MT` (measured
# 2026-09-10, the same convention that spells `cmd //c` in this tree). Unix
# builds are untouched: their optimisation flags live in `CMAKE_C_FLAGS` and
# survive.
#
# Two more settings serve the Vulkan runner, whose build nests a second CMake
# tree for ggml's shader generator. CMAKE_PROJECT_INCLUDE names the file
# beside this one that moves that tree to a short directory: from cargo's
# output directory its compiler check wrote an object file at 265 characters,
# past the 260 cl.exe enforces, and every Vulkan build died there (measured
# 2026-09-10, details in host_runner_ep_base.cmake). The generator is Ninja
# when the Visual Studio tools ship one: with MSBuild, the same nested tree
# hit the file tracker's own 260 limit and then ran its configure, build and
# install steps at once. Ninja runs one build graph, and it is the generator
# llama.cpp's own Windows builds use. Without Ninja the Visual Studio
# generator is kept, so a processor-only build still works as before.
host_runner_cmake_env() {
    case "${1:-$(host_desktop_triple)}" in
        *-pc-windows-msvc)
            export CMAKE_C_FLAGS_RELEASE="-MT -O2 -Ob2 -DNDEBUG"
            export CMAKE_CXX_FLAGS_RELEASE="-MT -O2 -Ob2 -DNDEBUG"
            export CMAKE_C_FLAGS_RELWITHDEBINFO="-MT -O2 -Ob1 -DNDEBUG -Zi"
            export CMAKE_CXX_FLAGS_RELWITHDEBINFO="-MT -O2 -Ob1 -DNDEBUG -Zi"
            CMAKE_PROJECT_INCLUDE="$(cygpath -m "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/host_runner_ep_base.cmake")"
            export CMAKE_PROJECT_INCLUDE
            local ninja_dir
            if ninja_dir="$(host_runner_ninja_dir)"; then
                export CMAKE_GENERATOR=Ninja
                case ":$PATH:" in
                    *":$ninja_dir:"*) ;;
                    *) export PATH="$ninja_dir:$PATH" ;;
                esac
            fi
            ;;
    esac
}

# Drop whisper.cpp's compiled library so the flags above are applied.
#
# cargo reruns a build script on the conditions that script declares, and
# whisper-rs-sys declares its wrapper header and a few SDK variables, none of
# which is a CMAKE_* flag. On a tree that already holds a runner, the release
# build kept the library it had, compiled without optimisation, and the flags
# above changed nothing. `cargo clean -p` on that one package is what makes
# the next build compile it again, at the cost of one whisper.cpp build, one
# and a half minutes on the bench with Ninja. A no-op on every other host.
#
# Arguments: the triple (or empty for the host), then the cargo arguments that
# select the artefacts, `--release` and `--target <triple>` as the build uses.
host_runner_cmake_clean() {
    local triple="${1:-$(host_desktop_triple)}"
    shift || true
    case "$triple" in
        *-pc-windows-msvc) cargo clean -p whisper-rs-sys "$@" ;;
    esac
}

# The directory holding ninja.exe: the PATH first, then the one the Visual
# Studio installer lays down with its "C++ CMake tools" component, located
# through vswhere, which every Visual Studio 2017 and later install carries.
# Prints nothing and returns 1 when there is none.
host_runner_ninja_dir() {
    local found
    if found="$(command -v ninja 2>/dev/null)" && [ -n "$found" ]; then
        dirname "$found"
        return 0
    fi
    local pf86 vswhere root
    pf86="$(printenv 'ProgramFiles(x86)' 2>/dev/null || true)"
    [ -n "$pf86" ] || return 1
    vswhere="$(cygpath -u "$pf86")/Microsoft Visual Studio/Installer/vswhere.exe"
    [ -x "$vswhere" ] || return 1
    root="$("$vswhere" -latest -products '*' -property installationPath 2>/dev/null | tr -d '\r')"
    [ -n "$root" ] || return 1
    found="$(cygpath -u "$root")/Common7/IDE/CommonExtensions/Microsoft/CMake/Ninja"
    [ -x "$found/ninja.exe" ] || return 1
    printf '%s' "$found"
}
