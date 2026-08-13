#!/usr/bin/env bash
# Make a linked worktree measure the same repository the main tree measures.
#
# Eight guards declared "local" answered a different verdict in a fresh worktree
# than in the main tree, and two of them answered a plausible wrong one rather
# than failing to start: `npx svelte-check` reported 2050 errors over 853 files
# instead of 1 over 4943, and `bash tests/cli/cli-e2e.sh` reported 153 failures
# out of 154 cases, which reads exactly like a product regression.
#
# Three of the four missing artefacts are now tracked by git and arrive with the
# checkout, so this script has only two things left to do:
#
#   rust  the `target/Resources/python` symlink, and the debug CLI binary that
#         tests/cli/cli-e2e.sh runs. The binary is linked against
#         `@executable_path/../Resources/python/lib/libpython3.13.dylib`, the
#         packaged layout. In a dev tree `@executable_path` is `target/debug`, so
#         dyld looks under `target/Resources`, which is born empty in a new
#         worktree, and every invocation dies before main(). The suite builds
#         nothing itself: it resolves `target/release/apollia-os` then
#         `target/debug/apollia-os` and exits 1 when it finds neither, so the
#         binary is part of preparing the tree and not part of running the guard.
#   ui    `npm ci` under crates/apollia-desktop/ui
#   docs  `npm ci` under docs/site
#
# Groups are cumulative and there is no default: `worktree-prep.sh` with no
# argument lists them and exits 1. A default would be a supposition, and this
# script exists because a supposition about an environment cost a wrong verdict.
#
# The Python bundle is not rebuilt. It is linked from the main working tree,
# which makes this worktree depend on a state of the main tree. That dependency
# is declared rather than hidden: the resolved path is printed here and recorded
# in the header of the verdict table produced by scripts/worktree_verdicts.py.
# Rebuilding instead costs a network round trip and 122 MB per worktree; copying
# instead costs 122 MB and is the escape hatch if the bundle is ever replaced
# under a running worktree.
#
# `just _bundle-python` is deliberately not reused: it resolves the bundle of
# the tree it runs in, and this script needs the bundle of the main working
# tree, which is the whole point of a linked worktree. The two now agree on
# what to answer when they find nothing, and both return 1.
#
# Usage:
#     bash scripts/worktree-prep.sh rust
#     bash scripts/worktree-prep.sh ui docs
#     bash scripts/worktree-prep.sh full
set -euo pipefail

TREE="$(git rev-parse --show-toplevel)"
cd "$TREE"

usage() {
    cat >&2 <<'EOF'
usage: bash scripts/worktree-prep.sh <group> [<group> ...]

Groups, cumulative, no default:

  rust   link target/Resources/python to the Python bundle of the main working
         tree. Required by cargo check, clippy, test and tests/cli/cli-e2e.sh.
  ui     npm ci under crates/apollia-desktop/ui. Required by npm run build,
         npx svelte-check and npx vitest run.
  docs   npm ci under docs/site. Required by the documentation build.
  full   all three.

There is no default group. Preparing what was not asked for costs 792 MB to a
lot that only touches Rust, and guessing is what this script exists to stop.
EOF
}

prep_rust() {
    echo "==> rust: linking the Python bundle"

    # Never a hardcoded path: the main working tree is resolved the way
    # scripts/check_guard_verdicts.py resolves it, and the triple the way
    # crates/apollia-desktop/scripts/bundle-cli.sh resolves it.
    local common main triple bundle
    common="$(git rev-parse --path-format=absolute --git-common-dir)"
    main="$(dirname "$common")"
    triple="$(rustc -vV | grep host | awk '{print $2}')"
    bundle="${main}/target/python-bundle/${triple}/python"

    if [ ! -x "${bundle}/bin/python3.13" ] && [ ! -x "${bundle}/python.exe" ]; then
        echo "ERROR: no Python bundle at ${bundle}" >&2
        echo "       Build one in the main working tree with:" >&2
        echo "       bash packaging/build-python-bundle.sh ${triple} target/python-bundle/${triple}" >&2
        return 1
    fi

    mkdir -p target/Resources
    ln -sfn "$bundle" target/Resources/python
    echo "    target/Resources/python -> ${bundle}"

    if [ -x target/release/apollia-os ] || [ -x target/debug/apollia-os ]; then
        echo "    apollia-os already built"
    else
        echo "==> rust: building apollia-os for tests/cli/cli-e2e.sh"
        cargo build -p apollia-cli
    fi
}

prep_ui() {
    echo "==> ui: npm ci under crates/apollia-desktop/ui"
    ( cd crates/apollia-desktop/ui && npm ci )
}

prep_docs() {
    echo "==> docs: npm ci under docs/site"
    ( cd docs/site && npm ci )
}

if [ "$#" -eq 0 ]; then
    usage
    exit 1
fi

WANT_RUST=0
WANT_UI=0
WANT_DOCS=0

for group in "$@"; do
    case "$group" in
        rust) WANT_RUST=1 ;;
        ui) WANT_UI=1 ;;
        docs) WANT_DOCS=1 ;;
        full) WANT_RUST=1; WANT_UI=1; WANT_DOCS=1 ;;
        *)
            echo "ERROR: unknown group '${group}'" >&2
            usage
            exit 1
            ;;
    esac
done

echo "tree: ${TREE}"
if [ "$WANT_RUST" -eq 1 ]; then prep_rust; fi
if [ "$WANT_UI" -eq 1 ]; then prep_ui; fi
if [ "$WANT_DOCS" -eq 1 ]; then prep_docs; fi

echo "==> done. Record the verdicts with:"
echo "    python3 scripts/worktree_verdicts.py --record <output.json>"
