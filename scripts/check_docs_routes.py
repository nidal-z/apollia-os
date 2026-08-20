#!/usr/bin/env python3
"""Fail when a documentation page is written but never rendered.

Docusaurus writes one output file per route. When a `_category_.json` declares
`link.type = "generated-index"` on the slug its own directory already serves
through `index.md`, the two routes land on the same output file and the
generated index wins. The hand-written page is still parsed, still listed in
the sidebar as a card, and never rendered as a page.

That matters beyond the lost prose. The build's link verifier collects the
links it finds while rendering, so a page that is never rendered has its links
collected from nowhere: a dead internal route written in it travels through
`npm run build` in exit 0, with `onBrokenLinks: 'throw'` set and working. Five
sections of this site were in that state, in both locales, which put ten source
pages and their links outside the reach of the only judge the repository has
for them.

What counts as a violation: a `_category_.json` that declares a `link` while
its directory holds an `index.md` or an `index.mdx`. What does not: a `link` in
a directory with no index page, which is the normal way to give a category a
generated landing page, and an index page in a directory whose `_category_.json`
declares no `link`, which is the convention that makes the index page the
category landing page.

What this does not catch, and it is deliberate: every other reason a source
page may fail to render, such as `draft: true` or a route collision introduced
by a plugin. That class is only observable after a build, and this guard reads
sources.

Exit codes:
    0  every category file read, no masked index page
    1  at least one index page is masked by its own category link
    2  nothing was measured, or a section that must be covered was not read

Usage:
    python3 scripts/check_docs_routes.py
"""

import json
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]

# The whole site tree, both locales. The French mirror lives under
# `i18n/fr/docusaurus-plugin-content-docs/current/` and carries its own copy of
# every `_category_.json`, so a guard rooted at `docs/site/docs` would report
# green on a site where half the sections are masked.
SCAN_ROOT = Path("docs/site")

# Derived or vendored output. `docs/reference/api` is not excluded: it is
# regenerated at build time and carries no `_category_.json` today, and if the
# generator ever emits one it should be judged like any other.
EXCLUDED_DIRS = {
    "node_modules",
    "build",
    ".docusaurus",
    "dist",
    ".git",
}

INDEX_NAMES = ("index.md", "index.mdx")

# The ten sections this guard was written for. Asserted positively so that
# narrowing SCAN_ROOT, or moving the French mirror, fails the run instead of
# quietly reporting green on a smaller set. A count alone would not do it: a
# walk that found many files proves it walked, not that it walked here.
REQUIRED_COVERAGE = [
    Path("docs/site/docs/explanation/_category_.json"),
    Path("docs/site/docs/how-to/_category_.json"),
    Path("docs/site/docs/operator-help/_category_.json"),
    Path("docs/site/docs/reference/_category_.json"),
    Path("docs/site/docs/tutorials/_category_.json"),
    Path("docs/site/i18n/fr/docusaurus-plugin-content-docs/current/explanation/_category_.json"),
    Path("docs/site/i18n/fr/docusaurus-plugin-content-docs/current/how-to/_category_.json"),
    Path("docs/site/i18n/fr/docusaurus-plugin-content-docs/current/operator-help/_category_.json"),
    Path("docs/site/i18n/fr/docusaurus-plugin-content-docs/current/reference/_category_.json"),
    Path("docs/site/i18n/fr/docusaurus-plugin-content-docs/current/tutorials/_category_.json"),
]


def iter_category_files(root: Path) -> list[Path]:
    """Return every `_category_.json` under `root`, derived trees excluded."""
    if not root.is_dir():
        return []
    files = []
    for path in root.rglob("_category_.json"):
        if not path.is_file():
            continue
        if EXCLUDED_DIRS.intersection(path.relative_to(root).parts):
            continue
        files.append(path)
    return sorted(set(files))


def index_sibling(category_file: Path) -> Path | None:
    """Return the index page that `category_file` sits next to, if any."""
    for name in INDEX_NAMES:
        candidate = category_file.parent / name
        if candidate.is_file():
            return candidate
    return None


def masked_pages(root: Path) -> list[tuple[Path, Path, str]]:
    """Return every index page a sibling category link takes the route from.

    Each entry is the category file, the index page it masks, and the `link`
    declaration that masks it, all as read from disk. Pure with respect to the
    tree it is given, so a caller can drive it on a fixture that violates the
    rule as well as on the repository, which is the only way to know the
    detector fires at all.
    """
    hits = []
    for category_file in iter_category_files(root):
        try:
            data = json.loads(category_file.read_text(encoding="utf-8"))
        except (OSError, ValueError) as exc:
            raise ValueError(f"{category_file}: {exc}") from exc
        link = data.get("link")
        if not link:
            continue
        index = index_sibling(category_file)
        if index is None:
            continue
        hits.append((category_file, index, json.dumps(link, sort_keys=True)))
    return hits


def uncovered_required(root: Path) -> list[Path]:
    """Return the sections that exist on disk but fell outside the walk."""
    scanned = set(iter_category_files(root))
    missing = []
    for rel in REQUIRED_COVERAGE:
        path = REPO_ROOT / rel
        if path.is_file() and path not in scanned:
            missing.append(rel)
    return missing


def main() -> int:
    root = REPO_ROOT / SCAN_ROOT
    files = iter_category_files(root)
    if not files:
        print(
            f"check_docs_routes: NO COVERAGE, no _category_.json under {SCAN_ROOT}",
            file=sys.stderr,
        )
        return 2

    missing = uncovered_required(root)
    if missing:
        print(
            "check_docs_routes: NO COVERAGE, these sections exist but were not "
            "read:",
            file=sys.stderr,
        )
        for rel in missing:
            print(f"  {rel}", file=sys.stderr)
        return 2

    try:
        hits = masked_pages(root)
    except ValueError as exc:
        print(f"check_docs_routes: unreadable category file, {exc}", file=sys.stderr)
        return 2

    with_index = sum(1 for path in files if index_sibling(path) is not None)
    print(
        f"check_docs_routes: {len(files)} category files read under {SCAN_ROOT}, "
        f"{with_index} next to an index page, {len(hits)} masked"
    )

    if hits:
        print(
            f"\n{len(hits)} index page(s) are written but never rendered. Their "
            f"category declares a link on the route the index page already "
            f"serves, so the generated page wins and the source page, with every "
            f"link in it, leaves the build's link verifier:\n",
            file=sys.stderr,
        )
        for category_file, index, link in hits:
            rel_cat = category_file.relative_to(REPO_ROOT)
            rel_idx = index.relative_to(REPO_ROOT)
            print(f"  {rel_cat}", file=sys.stderr)
            print(f"    masks {rel_idx}", file=sys.stderr)
            print(f"    link  {link}", file=sys.stderr)
        print(
            "\nRemove the `link` block. Docusaurus then takes the index page as "
            "the category landing page, and the sidebar entry keeps working.",
            file=sys.stderr,
        )
        return 1

    print("check_docs_routes: every index page owns its route")
    return 0


if __name__ == "__main__":
    sys.exit(main())
