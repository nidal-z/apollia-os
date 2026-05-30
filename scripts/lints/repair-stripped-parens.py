#!/usr/bin/env python3
"""
Réparation : restaure `()` sur les noms de méthodes/fonctions connus
qui ont été dépouillés par un pattern trop agressif.

Liste close de noms qui doivent toujours apparaître avec `()` quand
ils sont dans un contexte de méthode (suivis d'une mention sans `(`).

Patterns réparés :
  - `\.MethodName ` → `.MethodName() `
  - "  appeler `MethodName` " → "  appeler `MethodName()` "
  - "MethodName, " → si suivi d'une autre méthode, OK ; sinon laisser

Usage :
  python3 scripts/lints/repair-stripped-parens.py --dry-run docs/book/src docs/wiki
  python3 scripts/lints/repair-stripped-parens.py docs/book/src docs/wiki
"""

import argparse
import re
import sys
from pathlib import Path

# Méthodes/fonctions Apollia qui prennent toujours `()`.
KNOWN = [
    # AIP
    "manifest", "run", "on_start", "on_stop", "__init__",
    # ctx.tools
    "call", "describe", "list",
    # ctx.llm
    "complete", "chat", "stream", "stream_complete", "run_tools",
    # ctx.memory
    "search", "record", "recall_entry", "recall_all", "recall",
    # ContextBootstrap
    "is_stale", "run_bootstrap", "extra_scopes", "persist", "load_snapshot",
    "load_meta", "needs_bootstrap",
    # A2A
    "delegate", "a2a_invoke", "a2a_list_skills", "a2a_discover", "emit_token",
    # Pipeline / runtime
    "fire", "reload", "clear", "close", "invoke", "decide",
    "start", "stop", "submit", "spawn", "shutdown",
    # tests
    "build", "validate",
]

# Pattern : nom dans backticks suivi d'un caractère non-paren.
# Ex: `manifest` doit → `manifest()`
# Mais : `manifest()` reste tel quel (déjà avec parens)
def make_patterns():
    pats = []
    for name in sorted(set(KNOWN), key=len, reverse=True):
        # `name` sans `(` après (donc soit fin backtick, soit autre)
        # match: `name` quand suivi de espace/ponctuation, PAS si suivi de `(`
        # On ne touche que les occurrences en `…` (inline code).
        pat = re.compile(rf"`{re.escape(name)}`(?!\()")
        pats.append((pat, f"`{name}()`"))
    return pats


CODE_FENCE_RE = re.compile(r"^(\s*)(```|~~~)")


def process_text(text: str, patterns) -> tuple[str, int]:
    """N'opère que hors blocs code."""
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
            out.append(raw)
            continue

        new_line = raw
        for pat, repl in patterns:
            new_line, n = pat.subn(repl, new_line)
            changes += n
        out.append(new_line)

    return "".join(out), changes


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("paths", nargs="+")
    ap.add_argument("--dry-run", action="store_true")
    args = ap.parse_args()

    files: list[Path] = []
    for p in args.paths:
        path = Path(p)
        if path.is_dir():
            files.extend(sorted(path.rglob("*.md")))
        elif path.is_file() and path.suffix == ".md":
            files.append(path)

    patterns = make_patterns()
    total_files = 0
    total_changes = 0

    for f in files:
        text = f.read_text(encoding="utf-8")
        new_text, n = process_text(text, patterns)
        if new_text == text:
            continue
        if not args.dry_run:
            f.write_text(new_text, encoding="utf-8")
        total_files += 1
        total_changes += n
        print(f"{'(dry)' if args.dry_run else 'OK'}   {f}  ({n} réparations)")

    print(f"\nFichiers réparés : {total_files}")
    print(f"Réparations totales : {total_changes}")
    if args.dry_run:
        print("(dry-run)")


if __name__ == "__main__":
    main()
