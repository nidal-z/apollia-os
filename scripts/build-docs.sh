#!/usr/bin/env bash
# Master build des 3 sites de documentation Apollia.
#
# Sortie :
#   web/dist/book/   ← mdBook
#   web/dist/wiki/   ← VitePress (référence technique)
#   web/dist/help/   ← VitePress (centre d'aide opérateur)
#
# Variables d'environnement (URLs cross-site, défauts publics) :
#   BOOK_URL  (défaut: https://book.apollia.fr)
#   DOCS_URL  (défaut: https://docs.apollia.fr)
#   HELP_URL  (défaut: https://guide.apollia.fr)
#
# Usage :
#   ./scripts/build-docs.sh           # build tous les sites
#   ./scripts/build-docs.sh book      # build un seul site
#   ./scripts/build-docs.sh wiki help # build un sous-ensemble

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DIST="$ROOT/web/dist"
TARGETS=("${@:-book wiki help}")
TARGETS=(${TARGETS[@]})

mkdir -p "$DIST"

log()  { printf "\n\033[1;36m== %s ==\033[0m\n" "$1"; }
ok()   { printf "  \033[32m✓\033[0m %s\n" "$1"; }
fail() { printf "  \033[31m✗\033[0m %s\n" "$1"; exit 1; }

build_book() {
  log "Build mdBook (book/)"
  command -v mdbook >/dev/null 2>&1 || fail "mdbook non installé. cargo install mdbook"
  cd "$ROOT/book"
  mdbook build
  rm -rf "$DIST/book"
  mkdir -p "$DIST/book"
  cp -R "$ROOT/target/book/." "$DIST/book/"
  ok "→ $DIST/book"
}

build_vitepress() {
  local name="$1"
  log "Build VitePress ($name)"
  cd "$ROOT/web/${name}-site"
  if [ ! -d node_modules ]; then
    ok "Installation des dépendances npm…"
    npm install --silent --no-audit --no-fund
  fi
  npm run build --silent
  rm -rf "$DIST/$name"
  mkdir -p "$DIST/$name"
  cp -R "$ROOT/web/${name}-site/.vitepress/dist/." "$DIST/$name/"
  ok "→ $DIST/$name"
}

for t in "${TARGETS[@]}"; do
  case "$t" in
    book) build_book ;;
    wiki) build_vitepress wiki ;;
    help) build_vitepress help ;;
    *) fail "cible inconnue : $t (attendu: book, wiki, help)" ;;
  esac
done

log "Build terminé"
ls -la "$DIST" 2>/dev/null || true
