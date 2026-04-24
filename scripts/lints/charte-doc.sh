#!/usr/bin/env bash
# Linter charte documentaire — règles L1.4 du CONTEXTE-DEPART.md
# Usage : ./scripts/lints/charte-doc.sh [--strict]
# Retour : 0 = tout OK, 1 = violation détectée
#
# Applique 5 règles mécaniques :
#   R3  wiki : aucune page > 1500 lignes
#   R7  wiki : aucune capture d'écran
#   R8  book : aucune table markdown > 10 lignes (hors allow-list)
#   R10 wiki+help : aucune section H2 > 800 mots (warn-only)
#   NG  wiki : aucun titre narratif interdit (Tutoriel, Premiers pas, Quickstart, etc.)

set -uo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
WIKI="$ROOT/docs/wiki"
BOOK="$ROOT/book/src"
HELP="$ROOT/help"
STRICT="${1:-}"
FAIL=0

log_fail() { printf "\033[31mFAIL\033[0m  %s\n" "$1"; FAIL=1; }
log_warn() { printf "\033[33mWARN\033[0m  %s\n" "$1"; }
log_pass() { printf "\033[32mPASS\033[0m  %s\n" "$1"; }

header() { printf "\n\033[1m== %s ==\033[0m\n" "$1"; }

# ---------- R3 : wiki ≤ 1500 lignes ----------
header "R3 — Pages wiki ≤ 1500 lignes"
R3_VIOLATIONS=0
# Exclusions : index historique qui a vocation à grandir
R3_ALLOW=("Decisions-Log.md" "Sprint-Summary.md")
while IFS= read -r -d '' f; do
  base="$(basename "$f")"
  allow=0
  for a in "${R3_ALLOW[@]}"; do [[ "$base" == "$a" ]] && allow=1; done
  lines=$(wc -l < "$f")
  if (( lines > 1500 )); then
    if (( allow == 1 )); then
      log_warn "$base : $lines lignes (allow-list)"
    else
      log_fail "$base : $lines lignes (> 1500)"
      R3_VIOLATIONS=$((R3_VIOLATIONS+1))
    fi
  fi
done < <(find "$WIKI" -maxdepth 1 -name "*.md" -print0)
(( R3_VIOLATIONS == 0 )) && log_pass "R3 : aucune page wiki hors allow-list > 1500 lignes"

# ---------- R7 : wiki sans capture d'écran ----------
header "R7 — Aucune capture d'écran dans le wiki"

# Note : SVG autorisés (diagrammes architecture PlantUML / Mermaid)
# Interdit : captures d'écran UI (png, jpg, jpeg, gif, webp)
R7_HITS=$(grep -rlE '!\[[^]]*\]\([^)]+\.(png|jpg|jpeg|gif|webp)\)|<img[[:space:]]' "$WIKI" 2>/dev/null | head -20)
if [[ -n "$R7_HITS" ]]; then
  log_fail "R7 : captures détectées dans :"
  printf "%s\n" "$R7_HITS"
else
  log_pass "R7 : aucune capture d'écran dans le wiki"
fi

# ---------- R8 : book, aucune table > 10 lignes ----------
header "R8 — Book : tables markdown ≤ 10 lignes (hors allow-list)"
R8_VIOLATIONS=0
# Allow-list éducationnelle : chapitres "catalogue" où la table longue est la valeur pédagogique
R8_ALLOW=(
  "ch04-01-native-tools.md"     # catalogue pédagogique 10 outils
  "ch13-01-topology.md"          # DAG pipeline, éducatif
  "ch03-01-manifest.md"          # référence canonique AgentManifest
  "appendix-d-roadmap.md"        # roadmap historique
  "appendix-e-sprint-summary.md" # historique sprints
  "index.md"                     # index book
  "adr-031-i18n-svelte-i18n-fr-en.md" # matrice décision ADR
)
while IFS= read -r -d '' f; do
  base="$(basename "$f")"
  allow=0
  for a in "${R8_ALLOW[@]}"; do [[ "$base" == "$a" ]] && allow=1; done
  # Détecter les tables : séquences consécutives de lignes commençant par |
  # Utilise awk pour compter la plus longue séquence
  max_table=$(awk '
    /^[[:space:]]*\|/ { count++; if (count > max) max = count; next }
    { count = 0 }
    END { print max+0 }
  ' "$f")
  if (( max_table > 10 )); then
    if (( allow == 1 )); then
      log_warn "$base : table $max_table lignes (allow-list éducative)"
    else
      log_fail "$base : table de $max_table lignes (> 10)"
      R8_VIOLATIONS=$((R8_VIOLATIONS+1))
    fi
  fi
done < <(find "$BOOK" -name "*.md" -print0)
(( R8_VIOLATIONS == 0 )) && log_pass "R8 : aucune table book hors allow-list > 10 lignes"

# ---------- R10 : wiki/help, sections H2 ≤ 800 mots (warn) ----------
header "R10 — Sections H2 ≤ 800 mots (warn-only)"
R10_WARNS=0
check_r10() {
  local f="$1"
  # Split par H2, compte mots par section
  awk '
    /^## / {
      if (section != "") {
        if (count > 800) printf "%s  §%s : %d mots\n", file, section, count
      }
      section = $0; count = 0; next
    }
    { for (i=1; i<=NF; i++) count++ }
    END {
      if (count > 800) printf "%s  §%s : %d mots\n", file, section, count
    }
  ' file="$(basename "$f")" "$f"
}
for d in "$WIKI" "$HELP"; do
  [[ -d "$d" ]] || continue
  while IFS= read -r -d '' f; do
    out=$(check_r10 "$f")
    if [[ -n "$out" ]]; then
      log_warn "$out"
      R10_WARNS=$((R10_WARNS+1))
    fi
  done < <(find "$d" -name "*.md" -print0)
done
(( R10_WARNS == 0 )) && log_pass "R10 : aucune section H2 > 800 mots"

# ---------- NG : wiki sans titre narratif ----------
header "NG — Wiki sans titres narratifs (Tutoriel, Premiers pas, Quickstart…)"
# Patterns interdits dans H1/H2 du wiki (pages non-stub)
PATTERNS='^(# |## )(.*(Tutoriel|Premiers pas|Étape par étape|Comment démarrer|Pas à pas|Guide pas à pas))'
NG_VIOLATIONS=0
# Exclusions : pages légitimement narratives (installation operateur, pas agent)
NG_ALLOW=(
  "INSTALL-Quickstart.md"  # installation, pas tutoriel agent
  "INSTALL.md"
  "INSTALL-Production.md"
)
# Exclure les stubs (< 30 lignes) qui explicitement redirigent
while IFS= read -r -d '' f; do
  base="$(basename "$f")"
  allow=0
  for a in "${NG_ALLOW[@]}"; do [[ "$base" == "$a" ]] && allow=1; done
  (( allow == 1 )) && continue
  lines=$(wc -l < "$f")
  (( lines < 30 )) && continue
  if grep -nEi "$PATTERNS" "$f" > /tmp/charte-ng-hits 2>/dev/null; then
    if [[ -s /tmp/charte-ng-hits ]]; then
      log_fail "$(basename "$f") contient titre narratif :"
      cat /tmp/charte-ng-hits
      NG_VIOLATIONS=$((NG_VIOLATIONS+1))
    fi
  fi
done < <(find "$WIKI" -maxdepth 1 -name "*.md" -print0)
(( NG_VIOLATIONS == 0 )) && log_pass "NG : aucun titre narratif dans le wiki (hors stubs)"
rm -f /tmp/charte-ng-hits

# ---------- Résumé ----------
header "Résumé"
if (( FAIL == 0 )); then
  printf "\033[32mCharte L1.4 : toutes les règles strictes sont respectées.\033[0m\n"
  exit 0
else
  printf "\033[31mCharte L1.4 : %d violation(s) stricte(s) détectée(s).\033[0m\n" "$FAIL"
  exit 1
fi
