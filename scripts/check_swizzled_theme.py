#!/usr/bin/env python3
"""Hold the swizzled search theme against the plugin it was copied from.

`docs/site/src/theme/` carries six files ejected from
`@easyops-cn/docusaurus-search-local`. They exist for one reason: the plugin
writes `aria-label="Search"` as a literal on both of its inputs instead of
routing it through `translate()`, so a French screen reader announced the
English word while the visible placeholder read "Rechercher". A wrapper cannot
reach an attribute nested inside a component's own render, and setting it after
hydration would leave the served HTML wrong, so the components are ejected and
the attribute reads from the catalogue like every other string.

Ejecting the two leaves alone changes nothing: their parents import them by
relative path, which the `@theme/` alias does not intercept, so the parents are
ejected too. Moving a file out of the plugin breaks every relative import that
pointed inside it, so those are rewritten to the package path.

An ejected component is a fork, and a fork drifts. When the plugin ships a fix,
a feature or a security change in these files, nothing would say so: the site
would keep rendering the copy taken on the day of the eject. This guard is what
makes that visible.

It does not store a diff. It reads the upstream file, applies the two
deviations this tree declares, and requires the result to equal the swizzled
copy byte for byte. An upstream change therefore flows into the expected text
and only fails where it touches a region the eject changed, which is the only
place a human has to look.

Three exit codes, because a missing measurement must never read as a pass:

  0  every swizzled file equals its upstream original once the declared
     deviations are applied.
  1  at least one file diverges, named with the first differing line of each
     side.
  2  nothing measured: the plugin is not installed, so there is no upstream to
     compare against. Run `npm ci` in `docs/site` first.

Usage:
    python3 scripts/check_swizzled_theme.py
    python3 scripts/check_swizzled_theme.py --selftest
"""

from __future__ import annotations

import os
import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
SITE = REPO / "docs" / "site"
PACKAGE = "@easyops-cn/docusaurus-search-local"
UPSTREAM = SITE / "node_modules" / PACKAGE / "dist" / "client" / "client" / "theme"
SWIZZLED = SITE / "src" / "theme"

# The six files ejected. Every one is compared; the stylesheets are here because
# a swizzle whose CSS drifts silently stops being the component it replaced.
FILES = (
    "SearchBar/index.jsx",
    "SearchBar/SearchBar.jsx",
    "SearchBar/SearchBar.module.css",
    "SearchPage/index.js",
    "SearchPage/SearchPage.jsx",
    "SearchPage/SearchPage.module.css",
)

# Imports that must stay relative: they land on a file this tree also owns, so
# the copy has to use the copy. Addressed by path under the theme root.
OURS = {"SearchBar/index", "SearchBar/SearchBar", "SearchPage/index", "SearchPage/SearchPage"}

# The literal the plugin writes, and what this tree writes instead.
ARIA_UPSTREAM = 'aria-label="Search"'
ARIA_OURS = (
    'aria-label={translate({\n            id: "theme.SearchBar.label",\n'
    '            message: "Search",\n'
    '            description: "The ARIA label and placeholder for search button",\n'
    "        })}"
)

_IMPORT = re.compile(r'(from\s+"|import\s+")(\.[^"]+)(")')


def rewrite_imports(text: str, rel: str) -> str:
    """Point every escaping relative import at the package it came from.

    `rel` is the file's path under the theme root, which is where it used to
    live and therefore what its relative specifiers were written against.
    """
    here = (UPSTREAM / rel).parent

    def fix(m: re.Match[str]) -> str:
        target = os.path.normpath(os.path.join(str(here), m.group(2)))
        try:
            inside = str(Path(target).relative_to(UPSTREAM))
        except ValueError:
            inside = None
        if inside in OURS:
            return m.group(0)
        return f'{m.group(1)}{os.path.relpath(target, SITE / "node_modules")}{m.group(3)}'

    return _IMPORT.sub(fix, text)


def expected(upstream_text: str, rel: str) -> str:
    """The swizzled file this tree should hold, derived from the upstream one."""
    text = rewrite_imports(upstream_text, rel)
    return text.replace(ARIA_UPSTREAM, ARIA_OURS)


def first_difference(ours: str, theirs: str) -> str:
    """Name the first line where the two texts part company."""
    a, b = ours.split("\n"), theirs.split("\n")
    for i in range(max(len(a), len(b))):
        left = a[i] if i < len(a) else "<end of file>"
        right = b[i] if i < len(b) else "<end of file>"
        if left != right:
            return (
                f"line {i + 1}\n"
                f"      swizzled: {left.strip()!r}\n"
                f"      expected: {right.strip()!r}"
            )
    return "no line differs, only the trailing bytes"


def compare(rel: str) -> str | None:
    """Return a defect description, or None when the file is what it should be."""
    ours_path, theirs_path = SWIZZLED / rel, UPSTREAM / rel
    if not ours_path.exists():
        return f"{rel}: the swizzled copy is missing from docs/site/src/theme"
    if not theirs_path.exists():
        return f"{rel}: the plugin no longer ships this file, so the copy replaces nothing"

    ours = ours_path.read_text(encoding="utf-8")
    want = expected(theirs_path.read_text(encoding="utf-8"), rel)
    if ours == want:
        return None
    return f"{rel}: diverges from the plugin at {first_difference(ours, want)}"


def selftest() -> int:
    """Prove the derivation and the comparison both bite."""
    failures = 0

    # An upstream aria-label becomes the translated call, and nothing else moves.
    src = 'const a = 1;\n<input aria-label="Search" id="x"/>\n'
    got = expected(src, "SearchBar/SearchBar.jsx")
    if ARIA_OURS not in got or ARIA_UPSTREAM in got:
        print("selftest: the aria-label deviation was not applied")
        failures += 1
    if "const a = 1;" not in got:
        print("selftest: the derivation changed a line it should not touch")
        failures += 1

    # An import that escapes the theme is rewritten, one that lands on a file we
    # own stays relative.
    src = 'import x from "../searchByWorker";\nimport y from "./SearchBar";\n'
    got = rewrite_imports(src, "SearchBar/index.jsx")
    if PACKAGE not in got:
        print("selftest: an escaping import was left relative")
        failures += 1
    if 'from "./SearchBar"' not in got:
        print("selftest: an import onto our own copy was rewritten")
        failures += 1

    # A difference anywhere is located rather than merely reported.
    if "line 2" not in first_difference("a\nb\n", "a\nB\n"):
        print("selftest: the difference was not located")
        failures += 1

    if failures:
        return 1
    print("check_swizzled_theme --selftest: 5 assertions, every one holds")
    return 0


def main() -> int:
    if "--selftest" in sys.argv:
        return selftest()

    if not UPSTREAM.exists():
        print(
            "check_swizzled_theme: nothing measured. The plugin is not installed at\n"
            f"  {UPSTREAM}\n"
            "so there is no upstream to compare the swizzled copies against.\n"
            "Run `npm ci` in docs/site first.",
            file=sys.stderr,
        )
        return 2

    defects = [d for rel in FILES if (d := compare(rel))]
    if defects:
        print(
            f"{len(defects)} swizzled file(s) are not what the plugin plus this tree's\n"
            "declared deviations produce. A fork that drifts renders the version taken\n"
            "on the day it was made and nothing else says so. Read the upstream change,\n"
            "carry it across, and keep only the deviations named in this file.\n",
            file=sys.stderr,
        )
        for d in defects:
            print(f"  {d}", file=sys.stderr)
        return 1

    print(
        f"check_swizzled_theme: {len(FILES)} swizzled files equal the installed plugin "
        "once the aria-label and the package-path imports are applied"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
