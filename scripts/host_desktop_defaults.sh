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
