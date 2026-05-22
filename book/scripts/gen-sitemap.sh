#!/usr/bin/env bash
# Génère sitemap.xml + robots.txt dans le build mdBook.
#
# mdBook ne fournit pas de sitemap natif. Ce script parcourt les .html générés
# et produit un sitemap.xml W3C valide + un robots.txt qui le référence.
#
# Usage : bash book/scripts/gen-sitemap.sh [DOMAIN] [BUILD_DIR]
# Defaults : DOMAIN=book.apollia.fr, BUILD_DIR=../target/book/html
#
# À appeler après `mdbook build book` dans le pipeline CI/CD.

set -euo pipefail

DOMAIN="${1:-book.apollia.fr}"
# Match book/book.toml build-dir = "../target/book" (mdbook output direct,
# pas de sous-dossier html/).
BUILD_DIR="${2:-target/book}"
TODAY="$(date -u +%Y-%m-%d)"

if [[ ! -d "$BUILD_DIR" ]]; then
  echo "Erreur : $BUILD_DIR introuvable. As-tu lancé 'mdbook build' avant ?" >&2
  exit 1
fi

# Sitemap.xml
{
  echo '<?xml version="1.0" encoding="UTF-8"?>'
  echo '<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">'
  find "$BUILD_DIR" -name "*.html" -type f \
    -not -name "404.html" \
    -not -name "print.html" \
    | sort \
    | while read -r f; do
      rel="${f#$BUILD_DIR/}"
      # index.html → URL racine de son dossier
      if [[ "$rel" == "index.html" ]]; then
        loc="https://${DOMAIN}/"
        priority="1.0"
        changefreq="weekly"
      elif [[ "$(basename "$rel")" == "index.html" ]]; then
        loc="https://${DOMAIN}/${rel%/index.html}/"
        priority="0.8"
        changefreq="monthly"
      else
        loc="https://${DOMAIN}/${rel%.html}.html"
        priority="0.7"
        changefreq="monthly"
      fi
      echo "  <url>"
      echo "    <loc>${loc}</loc>"
      echo "    <lastmod>${TODAY}</lastmod>"
      echo "    <changefreq>${changefreq}</changefreq>"
      echo "    <priority>${priority}</priority>"
      echo "  </url>"
    done
  echo '</urlset>'
} > "$BUILD_DIR/sitemap.xml"

echo "✓ Sitemap généré : $BUILD_DIR/sitemap.xml ($(wc -l < "$BUILD_DIR/sitemap.xml") lignes)"

# Robots.txt (si absent, sinon laissé tel quel)
if [[ ! -f "$BUILD_DIR/robots.txt" ]]; then
  cat > "$BUILD_DIR/robots.txt" <<EOF
User-agent: *
Allow: /

# Apollia OS est open source : on laisse les crawlers indexer le book.
User-agent: GPTBot
Allow: /

User-agent: ClaudeBot
Allow: /

User-agent: Google-Extended
Allow: /

User-agent: PerplexityBot
Allow: /

User-agent: CCBot
Allow: /

Sitemap: https://${DOMAIN}/sitemap.xml
EOF
  echo "✓ Robots.txt créé : $BUILD_DIR/robots.txt"
fi
