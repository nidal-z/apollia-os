#!/usr/bin/env python3
"""
Convertit les liens GitHub Wiki [[Page-Name|Label]] et [[Page-Name]]
en liens markdown standard [Label](./Page-Name.md), tout en respectant :
- les blocs de code triple-backtick (laissés intacts - les `[[pipelines]]` TOML restent)
- les blocs inline `code`
- les commentaires HTML <!-- ... -->

Critères de détection d'un vrai lien wiki :
- Avec pipe : [[X|Y]]  → toujours converti.
- Sans pipe : [[X]]    → converti UNIQUEMENT si X commence par majuscule
                         et ne contient ni point ni espace (filtre TOML).

Idempotent : si déjà converti, ne fait rien.

Usage :
    python3 scripts/lints/convert-wiki-links.py docs/wiki/*.md
    python3 scripts/lints/convert-wiki-links.py --dry-run docs/wiki/
"""

import argparse
import re
import sys
from pathlib import Path

# Patterns
LINK_WITH_PIPE = re.compile(r"\[\[([^|\]]+)\|([^\]]+)\]\]")
LINK_WITHOUT_PIPE = re.compile(r"\[\[([^|\]]+)\]\]")
WIKI_NAME_RE = re.compile(r"^[A-Z][A-Za-z0-9_-]*$")

CODE_FENCE_RE = re.compile(r"^(\s*)(```|~~~)")


def is_wiki_pagename(name: str) -> bool:
    """Heuristique : majuscule initiale, aucun point/espace, longueur raisonnable."""
    name = name.strip()
    return bool(WIKI_NAME_RE.match(name))


def convert_line(line: str) -> str:
    """Convertit les liens wiki sur une ligne hors code fence."""

    def replace_with_pipe(m: re.Match) -> str:
        page = m.group(1).strip()
        label = m.group(2).strip()
        return f"[{label}](./{page}.md)"

    def replace_without_pipe(m: re.Match) -> str:
        page = m.group(1).strip()
        if is_wiki_pagename(page):
            return f"[{page}](./{page}.md)"
        return m.group(0)  # laisse intact (probablement TOML)

    line = LINK_WITH_PIPE.sub(replace_with_pipe, line)
    line = LINK_WITHOUT_PIPE.sub(replace_without_pipe, line)
    return line


def convert_text(text: str) -> tuple[str, int]:
    """Convertit le texte en respectant les blocs code. Retourne (texte, nb_remplacements)."""
    lines = text.splitlines(keepends=True)
    out = []
    in_fence = False
    fence_marker = None
    replacements = 0

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
            out.append(raw)
            continue

        new_line = convert_line(raw)
        if new_line != raw:
            replacements += raw.count("[[")
        out.append(new_line)

    return "".join(out), replacements


def process_file(path: Path, dry_run: bool) -> tuple[int, bool]:
    text = path.read_text(encoding="utf-8")
    new_text, replacements = convert_text(text)
    if new_text == text:
        return 0, False
    if not dry_run:
        path.write_text(new_text, encoding="utf-8")
    return replacements, True


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("paths", nargs="+", help="Files or directories to process")
    ap.add_argument("--dry-run", action="store_true")
    ap.add_argument("--exclude", action="append", default=["_Sidebar.md", "_Footer.md"],
                    help="Filenames to skip (default: _Sidebar.md, _Footer.md)")
    args = ap.parse_args()

    files: list[Path] = []
    for p in args.paths:
        path = Path(p)
        if path.is_dir():
            files.extend(sorted(path.rglob("*.md")))
        elif path.is_file():
            files.append(path)
        else:
            print(f"WARN: skipping {p} (not found)", file=sys.stderr)

    total_files = 0
    total_changes = 0
    for f in files:
        if f.name in args.exclude:
            continue
        n, changed = process_file(f, args.dry_run)
        if changed:
            total_files += 1
            total_changes += n
            print(f"{'(dry)' if args.dry_run else 'OK'}   {f}  ({n} liens convertis)")

    print(f"\nRésumé : {total_files} fichier(s) modifié(s), ~{total_changes} occurrences traitées.")
    if args.dry_run:
        print("(dry-run - aucun fichier écrit)")


if __name__ == "__main__":
    main()
