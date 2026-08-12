#!/usr/bin/env bash
#
# Install the seed ecosystem into the REAL ~/.apollia, for a manual screenshot
# session, and put the previous one aside so it can come back untouched.
#
# The automation recipes swap HOME to a throwaway directory, which is right for
# an unattended run and wrong for a human one: you want to click through the
# real application, with its real window and its real menus. So this takes the
# other approach, moving the real profile out of the way rather than hiding it.
#
#   bash tests/cli/seed/load.sh      # back up, then install the seed
#   bash tests/cli/seed/unload.sh    # restore what was there before
#
# The backup is a move, not a copy: nothing is duplicated, and a half-finished
# load cannot leave two profiles claiming to be the same one. If a backup
# already exists, this refuses rather than overwrite it, because that backup is
# someone's real data and the second load would be the one that destroys it.
#
# This is the human path, so it turns the narrative overlay ON by default: the
# checked-in seed is a test fixture sized for assertions, the overlay is the
# story the screenshots tell. The automated path (build-seed.sh called by CI and
# by the just recipes) leaves it OFF, so the fixture those runs assert against
# never changes under them.
#
# Environment:
#   APOLLIA_SEED_OVERLAY       overlay directory. Defaults to
#                              ~/.apollia-seed-overlay when that exists, to
#                              nothing when it does not. Set it to a path that
#                              does not exist and build-seed.sh stops rather than
#                              build a fixture that looks right and is not.
#   APOLLIA_SEED_PROJECT_ROOT  what the seeded project rows point at. Defaults to
#                              this checkout. Override it before a public
#                              screenshot run: that path is displayed on the
#                              project pages, the permission cards and the audit
#                              trail, and it is your machine's path.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
APOLLIA_DIR="${APOLLIA_HOME:-$HOME/.apollia}"
BACKUP_DIR="${APOLLIA_DIR}.before-seed"
DEFAULT_OVERLAY="$HOME/.apollia-seed-overlay"

if [ -z "${APOLLIA_SEED_OVERLAY:-}" ] && [ -d "$DEFAULT_OVERLAY" ]; then
  export APOLLIA_SEED_OVERLAY="$DEFAULT_OVERLAY"
fi

if [ -e "$BACKUP_DIR" ]; then
  echo "error: a backup already exists at $BACKUP_DIR" >&2
  echo "" >&2
  echo "That means a seed is probably already loaded. Run unload.sh first." >&2
  echo "If you are sure the backup is stale, move it away by hand; this script" >&2
  echo "will not overwrite it, because it may be the only copy of a real profile." >&2
  exit 1
fi

if [ -e "$APOLLIA_DIR" ]; then
  echo "==> moving your profile aside"
  echo "    $APOLLIA_DIR"
  echo " -> $BACKUP_DIR"
  mv "$APOLLIA_DIR" "$BACKUP_DIR"
else
  echo "==> no existing profile at $APOLLIA_DIR, nothing to back up"
  # Leave a marker so unload knows there was nothing here, and removes the seed
  # instead of restoring an empty directory over it.
  mkdir -p "$BACKUP_DIR"
  touch "$BACKUP_DIR/.was-absent"
fi

echo "==> building the seed into $APOLLIA_DIR"
STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT
# The build happens in a staging directory because build-seed.sh starts by
# wiping its target, and its target here would be your home. The rows must still
# name the home they will END UP under, not the staging path that this script
# deletes on the way out, hence the alias.
APOLLIA_SEED_HOME_ALIAS="$(dirname "$APOLLIA_DIR")" \
  bash "$HERE/build-seed.sh" "$STAGE" >/dev/null
mkdir -p "$(dirname "$APOLLIA_DIR")"
# Only `.apollia` moves. build-seed.sh also drops a copy of apollia.toml under
# the staging `.config/apollia/`, which is deliberately left behind: the desktop
# resolves `~/.apollia/apollia.toml` first (main.rs), that copy travels inside
# the directory being moved, and writing into the operator's real
# `~/.config/apollia` would be a side effect unload.sh could not undo.
mv "$STAGE/.apollia" "$APOLLIA_DIR"

# Check the seed that was just put in place, not the profile that used to be
# here: that one has been moved to $BACKUP_DIR fifteen lines above. This is the
# moment the defect the checker exists for is born, since the rows were baked in
# the staging directory that this script deletes on its way out.
if ! bash "$HERE/self-test-paths.sh" "$APOLLIA_DIR"; then
  echo "" >&2
  echo "The seed that was just loaded names paths that do not exist." >&2
  echo "Your own profile is intact at $BACKUP_DIR." >&2
  echo "Put it back with: bash tests/cli/seed/unload.sh" >&2
  exit 1
fi

echo ""
echo "Seed loaded. Launch the desktop application normally."
if [ -n "${APOLLIA_SEED_OVERLAY:-}" ]; then
  echo "Narrative overlay: ${APOLLIA_SEED_OVERLAY}"
else
  echo "Narrative overlay: none. Only the checked-in fixture is loaded, so the"
  echo "pages that the overlay fills (timeline, plans, agentic conversations)"
  echo "will show their thin or empty state. See seed/README.md, section Overlay."
fi
echo ""
echo "Two things to know before you shoot:"
echo "  - Timestamps are relative to the moment of this build, so the timeline"
echo "    and the audit trail have entries inside their default windows. Reload"
echo "    the seed if you come back to it days later."
echo "  - The inbox pending list cannot be seeded: it lives in the runtime's"
echo "    memory, not in a database. To photograph it, provoke an approval."
echo ""
echo "When you are done: bash tests/cli/seed/unload.sh"
