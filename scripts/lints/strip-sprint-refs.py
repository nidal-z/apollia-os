#!/usr/bin/env python3
"""
Retire les références sprint/story/epic des sources documentaires publiques
(book, wiki, help). Préserve les pages dont la raison d'être EST l'historique
sprint (Sprint-Summary.md, Decisions-Log.md, Roadmap.md, appendix-e).

Patterns retirés :
  - " (Sprint 12)"             → ""
  - " (Sprint 12, ADR-008)"    → " (ADR-008)"
  - " [Sprint 20]"             → ""
  - "*(Sprint 12, ADR-008)*"   → "*(ADR-008)*"
  - "*(Sprint 12)*"            → ""
  - " - Sprint 11"             → ""
  - "Sprint 11 - "             → "" (en titre)
  - " STORY-097"               → ""
  - "// Sprint 9 - Triggers"   → "// Triggers"
  - "// HITL Sprint 11"        → "// HITL"

Préserve les blocs ```...``` (le code Rust/Python comportant des commentaires
sprint reste touché en surface uniquement - voir CODE_AWARE).

Usage :
  python3 scripts/lints/strip-sprint-refs.py --dry-run docs/book/src docs/wiki help
  python3 scripts/lints/strip-sprint-refs.py docs/book/src docs/wiki help
"""

import argparse
import re
import sys
from pathlib import Path

# Pages à PAS toucher (l'historique sprint est leur raison d'être).
SKIP_FILES = {
    "Sprint-Summary.md",
    "Decisions-Log.md",
    "Roadmap.md",
    "appendix-e-sprint-summary.md",
    "appendix-d-roadmap.md",
}

# Skip aussi les fichiers ADR (l'historique sprint EST le contexte).
SKIP_PREFIXES = ("adr-",)

# Si CODE_AWARE = True : ne touche pas l'intérieur des blocs ```...```
# Si False : applique partout (utile car les commentaires Rust dans les
# exemples de code wiki contiennent souvent "// Sprint 11" qu'on veut
# nettoyer aussi).
# Pour book : on veut garder les exemples Python intacts.
# Pour wiki : on veut nettoyer même les commentaires Rust.

def _capitalize_next(m: re.Match) -> str:
    """Capitalise le caractère qui suit immédiatement la suppression."""
    rest = m.group("rest") if "rest" in m.groupdict() else ""
    if rest and rest[0].islower():
        return rest[0].upper() + rest[1:]
    return rest


PATTERNS_OUTSIDE_CODE = [
    # ── Phrases idiomatiques avec préposition (à traiter en premier, plus larges)
    # "Depuis le Sprint 13, mot..." → "Mot..." (capitalise)
    (re.compile(r"[Dd]epuis(?:\s+le)?\s+Sprint \d+\s*,?\s*(?P<rest>\S?)"), _capitalize_next),
    # "Avant le Sprint NN" / "Après le Sprint NN" → ""
    (re.compile(r"[AÀ](?:vant|près)(?:\s+le)?\s+Sprint \d+\s*,?\s*(?P<rest>\S?)"), _capitalize_next),
    # "À partir de Sprint NN, " / "À partir du Sprint NN" → ""
    (re.compile(r"À partir d[ue]\s+Sprint \d+\s*,?\s*(?P<rest>\S?)"), _capitalize_next),
    # "au Sprint NN" / "du Sprint NN" / "le Sprint NN"
    (re.compile(r"\s+(?:au|du|le)\s+Sprint \d+\b"), ""),
    # "comportement par défaut avant Sprint 37" → "comportement par défaut historique"
    (re.compile(r"avant\s+Sprint \d+"), "historique"),

    # ── Parenthèses et notes d'aside
    # "*(Sprint 8 + 28)*" / "*(Sprint 24 + 28)*" → ""
    (re.compile(r"\s*\*\(Sprint \d+(?:\s*\+\s*\d+)+\)\*"), ""),
    # "*(Sprint 12, ADR-008)*" → "*(ADR-008)*"
    (re.compile(r"\*\(Sprint \d+,\s*(ADR-\d+)\)\*"), r"*(\1)*"),
    # "*(Sprint 12)*" → ""
    (re.compile(r"\s*\*\(Sprint \d+\)\*"), ""),
    # "**Sprint 36 :**" en début de paragraphe → "**Note :**"
    (re.compile(r"\*\*Sprint \d+\s*:\*\*"), "**Note :**"),
    # "**Sprint 40** : Ajout..." (puce changelog) → "" (toute la ligne traitée
    # par les règles suivantes : la puce devient "- Ajout...")
    (re.compile(r"\*\*Sprint \d+\*\*\s*:\s*"), ""),
    # "← nouveau Sprint 8" / "← Sprint 8" en commentaire → ""
    (re.compile(r"\s*←\s*(?:nouveau\s+)?Sprint \d+"), ""),
    # " (Sprint 12, ADR-008)" → " (ADR-008)"
    (re.compile(r"\s*\(Sprint \d+,\s*(ADR-\d+)\)"), r" (\1)"),
    # " (Sprint 12, STORY-NNN)" → ""
    (re.compile(r"\s*\(Sprint \d+,\s*STORY-\d+\)"), ""),
    # " (STORY-NNN, ADR-XXX)" → " (ADR-XXX)"
    (re.compile(r"\s*\(STORY-\d+,\s*(ADR-\d+)\)"), r" (\1)"),
    # " (Sprint 12)" → ""
    (re.compile(r"\s*\(Sprint \d+\)"), ""),
    # " (Sprint 12 - feature flag X)" → " (feature flag X)"
    (re.compile(r"\(Sprint \d+\s*[-–-]\s*([^)]+)\)"), r"(\1)"),
    # " [Sprint 20]" → ""
    (re.compile(r"\s*\[Sprint \d+\]"), ""),
    # " [Sprint 9, CRUD Sprint 17]" / " [Sprint X, Sprint Y]" → ""
    (re.compile(r"\s*\[Sprint \d+(?:[,\+]\s*(?:CRUD\s+|nouveau\s+)?Sprint \d+)+\]"), ""),
    # "[Sprint 9, CRUD Sprint 17]" multi en commentaire shell/TOML
    (re.compile(r"\[Sprint \d+,\s*[A-Z]+\s+Sprint \d+\]"), ""),
    # "(Sprint 40+)" / "(Sprint 40 nouveau)" → ""
    (re.compile(r"\s*\(Sprint \d+\+?(?:\s+[a-zéèà]+)?\)"), ""),
    # " (depuis Sprint 28)" → ""
    (re.compile(r"\s*\(depuis Sprint \d+\)"), ""),

    # ── Tirets et titres
    # " - Sprint 11" / " – Sprint 11" / " - Sprint 11" → ""
    (re.compile(r"\s*[-–-]\s*Sprint \d+(\s*\([^)]*\))?"), ""),
    # Titre "Sprint 11 - " ou "## Sprint 11 - " → ""
    (re.compile(r"(^|\n)(#{1,6}\s+)Sprint \d+\s*[-–-]\s*", re.MULTILINE),
     lambda m: m.group(1) + m.group(2)),

    # ── STORY refs
    (re.compile(r"\bHITL STORY-\d+\b"), "HITL"),
    (re.compile(r"\s*\(STORY-\d+\)"), ""),
    # "(STORY-451, implémentation...)" → "(implémentation...)"
    (re.compile(r"\(STORY-\d+,\s*"), "("),
    (re.compile(r"\s+STORY-\d+,?"), ""),

    # ── Standalone fallbacks (à la fin)
    # "Sprint 9, " début de phrase
    (re.compile(r"(?<!\w)Sprint \d+,\s*"), ""),
    # " Sprint 41" milieu de phrase
    (re.compile(r"\s+Sprint \d+\b"), ""),
    # "Sprint 41" en début de ligne sans markup
    (re.compile(r"^Sprint \d+\b\s*", re.MULTILINE), ""),

    # ── Cleanup post-traitement
    # double virgule, virgule orpheline en début de phrase
    (re.compile(r",\s*,"), ","),
    (re.compile(r"\(\s*,\s*"), "("),
    (re.compile(r"\s*,\s*\)"), ")"),
    # parenthèses vides ORPHELINES uniquement (précédées d'un espace).
    # Ne pas matcher `manifest()` ni `run()` etc.
    (re.compile(r"\s+\(\s*\)"), ""),
    # espace avant fermeture parenthèse - uniquement si plusieurs (= " )"),
    # pour ne pas casser des cas légitimes.
    (re.compile(r"  +\)"), ")"),
    # espace avant ponctuation finale (uniquement . et , - préserver les espaces
    # insécables avant ; : ! ? typo française).
    (re.compile(r" +([.,])"), r"\1"),
]

# À l'intérieur des blocs code, nettoyage des commentaires + annotations.
PATTERNS_INSIDE_CODE = [
    # "// Sprint 9 - Triggers" → "// Triggers"
    (re.compile(r"//\s*Sprint \d+\s*[-–-]\s*"), "// "),
    # "// HITL Sprint 11" → "// HITL"
    (re.compile(r"\bHITL Sprint \d+\b"), "HITL"),
    # "// Sprint 11" en fin de ligne → ""
    (re.compile(r"\s*//\s*Sprint \d+\s*$", re.MULTILINE), ""),
    # " - Sprint 13" / " - Sprint 13" en commentaire → ""
    (re.compile(r"\s+[-–-]\s+Sprint \d+\b"), ""),
    # "[Sprint 20]" → ""
    (re.compile(r"\s*\[Sprint \d+\]"), ""),
    # "← Sprint 9" / "← Sprint 9 (HMAC-SHA256)" dans diagrammes ASCII → ""
    (re.compile(r"\s*←\s*Sprint \d+(?:\s*\([^)]*\))?"), ""),
    # "# Sprint NN" en commentaire TOML/shell → ""
    (re.compile(r"\s*#\s*Sprint \d+\s*$", re.MULTILINE), ""),
    # ", Sprint NN)" en milieu de parenthèse → ")"
    (re.compile(r",\s*Sprint \d+\)"), ")"),
    # "comportement par défaut avant Sprint 37" → "comportement par défaut historique"
    (re.compile(r"\s+avant Sprint \d+"), " historique"),
    # "*(Sprint 28, ADR-008)*" → "*(ADR-008)*"
    (re.compile(r"\*\(Sprint \d+,\s*(ADR-\d+)\)\*"), r"*(\1)*"),
    # "(Sprint NN, ADR-XXX)" → "(ADR-XXX)"
    (re.compile(r"\s*\(Sprint \d+,\s*(ADR-\d+)\)"), r" (\1)"),
    # "(Sprint NN)" → ""
    (re.compile(r"\s*\(Sprint \d+\)"), ""),
    # "(STORY-NNN)" → ""
    (re.compile(r"\s*\(STORY-\d+\)"), ""),
    # "STORY-NNN" → ""
    (re.compile(r"\s+STORY-\d+"), ""),
]

CODE_FENCE_RE = re.compile(r"^(\s*)(```|~~~)")


def process_text(text: str, code_aware: bool) -> tuple[str, int]:
    """Retourne (nouveau_texte, nb_changements)."""
    if not code_aware:
        # Mode simple : applique tout partout
        new = text
        changes = 0
        for pat, repl in PATTERNS_OUTSIDE_CODE + PATTERNS_INSIDE_CODE:
            new, n = pat.subn(repl, new)
            changes += n
        return new, changes

    # Mode code-aware : sépare blocs code / texte
    lines = text.splitlines(keepends=True)
    out = []
    in_fence = False
    fence_marker = None
    changes = 0

    for raw in lines:
        m = CODE_FENCE_RE.match(raw)
        if m:
            marker = m.group(2)
            if not in_fence:
                in_fence = True
                fence_marker = marker
            elif marker == fence_marker:
                in_fence = False
                fence_marker = None
            out.append(raw)
            continue

        if in_fence:
            new_line = raw
            for pat, repl in PATTERNS_INSIDE_CODE:
                new_line, n = pat.subn(repl, new_line)
                changes += n
            out.append(new_line)
        else:
            new_line = raw
            for pat, repl in PATTERNS_OUTSIDE_CODE:
                new_line, n = pat.subn(repl, new_line)
                changes += n
            out.append(new_line)

    return "".join(out), changes


def should_skip(path: Path) -> bool:
    if path.name in SKIP_FILES:
        return True
    if any(path.name.startswith(p) for p in SKIP_PREFIXES):
        return True
    return False


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("paths", nargs="+", help="Files or directories")
    ap.add_argument("--dry-run", action="store_true")
    ap.add_argument("--no-code-aware", action="store_true",
                    help="Désactive la préservation des blocs code (applique partout)")
    args = ap.parse_args()

    files: list[Path] = []
    for p in args.paths:
        path = Path(p)
        if path.is_dir():
            files.extend(sorted(path.rglob("*.md")))
        elif path.is_file() and path.suffix == ".md":
            files.append(path)

    total_files = 0
    total_changes = 0
    skipped = 0
    code_aware = not args.no_code_aware

    for f in files:
        if should_skip(f):
            skipped += 1
            continue
        text = f.read_text(encoding="utf-8")
        new_text, n = process_text(text, code_aware)
        if new_text == text:
            continue
        if not args.dry_run:
            f.write_text(new_text, encoding="utf-8")
        total_files += 1
        total_changes += n
        print(f"{'(dry)' if args.dry_run else 'OK'}   {f}  ({n} changements)")

    print(f"\nFichiers modifiés : {total_files}")
    print(f"Changements totaux : {total_changes}")
    print(f"Fichiers skippés (historique) : {skipped}")
    if args.dry_run:
        print("(dry-run)")


if __name__ == "__main__":
    main()
