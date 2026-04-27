#!/usr/bin/env bash
# Recrée les symlinks des sites VitePress après ajout/suppression de fichiers
# dans docs/wiki/ ou docs/help/.
#
# Usage : ./scripts/refresh-symlinks.sh [wiki|help|all]

set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TARGET="${1:-all}"

refresh_wiki() {
  local content="$ROOT/web/wiki-site/content"
  rm -rf "$content"
  mkdir -p "$content"
  for f in "$ROOT/docs/wiki"/*.md; do
    [ -f "$f" ] || continue
    ln -sfn "$f" "$content/$(basename "$f")"
  done
  echo "wiki : $(ls "$content" | wc -l) symlinks"
}

refresh_help() {
  local content="$ROOT/web/help-site/content"
  rm -rf "$content"
  mkdir -p "$content"
  for d in "$ROOT/docs/help"/*/; do
    [ -d "$d" ] || continue
    ln -sfn "$d" "$content/$(basename "$d")"
  done
  if [ -f "$ROOT/docs/help/index.md" ]; then
    ln -sfn "$ROOT/docs/help/index.md" "$content/index.md"
  fi
  echo "help : $(ls "$content" | wc -l) entries (dossiers + index)"
}

case "$TARGET" in
  wiki) refresh_wiki ;;
  help) refresh_help ;;
  all)  refresh_wiki; refresh_help ;;
  *) echo "Usage: $0 [wiki|help|all]"; exit 1 ;;
esac

echo "Symlinks refresh OK."
