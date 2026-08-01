#!/usr/bin/env bash
#
# Put back the profile that load.sh moved aside, and remove the seed.
#
#   bash scripts/automation/seed/unload.sh
#
# Quit the desktop application first. SQLite in WAL mode keeps -wal and -shm
# files next to each database, and moving a directory out from under a live
# process leaves it writing into a path that no longer exists.
set -euo pipefail

APOLLIA_DIR="${APOLLIA_HOME:-$HOME/.apollia}"
BACKUP_DIR="${APOLLIA_DIR}.before-seed"

if [ ! -e "$BACKUP_DIR" ]; then
  echo "error: no backup at $BACKUP_DIR" >&2
  echo "" >&2
  echo "Nothing was ever moved aside, so there is nothing to restore. If a seed" >&2
  echo "is loaded, it is now indistinguishable from a real profile and this" >&2
  echo "script will not guess which one to delete." >&2
  exit 1
fi

if pgrep -f 'apollia-desktop' >/dev/null 2>&1; then
  echo "error: the desktop application is still running" >&2
  echo "       quit it first, then run this again" >&2
  exit 1
fi

if [ -e "$APOLLIA_DIR" ]; then
  echo "==> removing the seed at $APOLLIA_DIR"
  rm -rf "$APOLLIA_DIR"
fi

if [ -e "$BACKUP_DIR/.was-absent" ]; then
  echo "==> there was no profile before the seed, leaving none behind"
  rm -rf "$BACKUP_DIR"
else
  echo "==> restoring your profile"
  echo "    $BACKUP_DIR"
  echo " -> $APOLLIA_DIR"
  mv "$BACKUP_DIR" "$APOLLIA_DIR"
fi

echo ""
echo "Done. Your profile is back as it was."
