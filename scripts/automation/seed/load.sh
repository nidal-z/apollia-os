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
#   bash scripts/automation/seed/load.sh      # back up, then install the seed
#   bash scripts/automation/seed/unload.sh    # restore what was there before
#
# The backup is a move, not a copy: nothing is duplicated, and a half-finished
# load cannot leave two profiles claiming to be the same one. If a backup
# already exists, this refuses rather than overwrite it, because that backup is
# someone's real data and the second load would be the one that destroys it.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
APOLLIA_DIR="${APOLLIA_HOME:-$HOME/.apollia}"
BACKUP_DIR="${APOLLIA_DIR}.before-seed"

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
bash "$HERE/build-seed.sh" "$STAGE" >/dev/null
mkdir -p "$(dirname "$APOLLIA_DIR")"
mv "$STAGE/.apollia" "$APOLLIA_DIR"

echo ""
echo "Seed loaded. Launch the desktop application normally."
echo ""
echo "Two things to know before you shoot:"
echo "  - Timestamps are relative to the moment of this build, so the timeline"
echo "    and the audit trail have entries inside their default windows. Reload"
echo "    the seed if you come back to it days later."
echo "  - The inbox pending list cannot be seeded: it lives in the runtime's"
echo "    memory, not in a database. To photograph it, provoke an approval."
echo ""
echo "When you are done: bash scripts/automation/seed/unload.sh"
