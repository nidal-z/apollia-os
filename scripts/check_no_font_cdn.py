#!/usr/bin/env python3
"""Fail the build when a web font would be fetched from a third party.

Apollia sells one promise above the others: a fresh launch contacts nobody. A
single `<link rel="stylesheet" href="https://fonts.googleapis.com/...">`, or a
CSS `@import url(https://...)`, breaks it before the user has clicked anything.
It leaks the IP, the User-Agent, the Accept-Language and the launch cadence of
every user, to a third party, on every cold start.

The rule held in the desktop app by comment and by the Tauri CSP, and it was
already reintroduced once and reverted. The documentation site has no CSP at
all, so nothing but this check stands between a copy-pasted Google Fonts
snippet and a published page that phones home.

What counts as a violation: any absolute `http(s)://` URL, in a scanned source
file, that points at a known font host or at a font file extension. What does
not: a bundled `@fontsource/...` import, which resolves inside `node_modules`
and is served from the site's own origin.

Usage:
    python3 scripts/check_no_font_cdn.py
"""

import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]

# Trees whose sources end up in something a user loads. `node_modules` and the
# built output are excluded: they are derived, and the derived output is what
# this rule protects, not what it inspects.
SCAN_ROOTS = [
    Path("docs/site/src"),
    Path("docs/site/docs"),
    Path("docs/site/i18n"),
    Path("docs/site/static"),
    Path("crates/apollia-desktop/ui/src"),
]

SCAN_FILES = [
    Path("docs/site/docusaurus.config.js"),
    Path("crates/apollia-desktop/ui/index.html"),
]

SCAN_SUFFIXES = {".css", ".scss", ".html", ".js", ".jsx", ".ts", ".tsx", ".svelte", ".md", ".mdx"}

EXCLUDED_DIRS = {"node_modules", "build", ".docusaurus", "dist", ".git"}

FONT_HOSTS = (
    "fonts.googleapis.com",
    "fonts.gstatic.com",
    "use.typekit.net",
    "p.typekit.net",
    "use.fontawesome.com",
    "fonts.bunny.net",
    "cdn.jsdelivr.net/npm/@fontsource",
    "cdn.jsdelivr.net/fontsource",
    "fontlibrary.org",
    "fast.fonts.net",
)

FONT_EXTENSIONS = (".woff2", ".woff", ".ttf", ".otf", ".eot")

_URL = re.compile(r"https?://[^\s\"'`)>]+")


def offending_urls(text: str) -> list[str]:
    """Return every absolute URL in `text` that would fetch a font remotely.

    Pure, so the self-test can drive it on both a violating and a clean sample
    without touching the filesystem. A detector that is only ever run on the
    real tree, and the real tree is clean, is a detector nobody has tested.
    """
    hits = []
    for url in _URL.findall(text):
        lowered = url.lower()
        if any(host in lowered for host in FONT_HOSTS):
            hits.append(url)
            continue
        # Strip a query string before looking at the extension: a font URL
        # commonly ends in `.woff2?v=3`.
        path = lowered.split("?", 1)[0].split("#", 1)[0]
        if path.endswith(FONT_EXTENSIONS):
            hits.append(url)
    return hits


def iter_files() -> list[Path]:
    files: list[Path] = []
    for root in SCAN_ROOTS:
        base = REPO_ROOT / root
        if not base.is_dir():
            continue
        for path in base.rglob("*"):
            if not path.is_file() or path.suffix.lower() not in SCAN_SUFFIXES:
                continue
            if EXCLUDED_DIRS.intersection(path.relative_to(REPO_ROOT).parts):
                continue
            files.append(path)
    for rel in SCAN_FILES:
        path = REPO_ROOT / rel
        if path.is_file():
            files.append(path)
    return sorted(set(files))


def main() -> int:
    files = iter_files()
    if not files:
        # A rule that examined nothing is not a rule that holds.
        print("check_no_font_cdn: NO COVERAGE, no source file matched", file=sys.stderr)
        return 1

    violations: list[str] = []
    for path in files:
        try:
            text = path.read_text(encoding="utf-8", errors="ignore")
        except OSError as exc:
            print(f"check_no_font_cdn: cannot read {path}: {exc}", file=sys.stderr)
            return 1
        for lineno, line in enumerate(text.splitlines(), start=1):
            for url in offending_urls(line):
                rel = path.relative_to(REPO_ROOT)
                violations.append(f"{rel}:{lineno}: {url}")

    print(f"check_no_font_cdn: {len(files)} files scanned")
    if violations:
        print(
            f"\n{len(violations)} remote font reference(s). Fonts must be bundled "
            f"(@fontsource) and served from the product's own origin:\n",
            file=sys.stderr,
        )
        for violation in violations:
            print(f"  {violation}", file=sys.stderr)
        return 1
    print("check_no_font_cdn: no remote font reference")
    return 0


if __name__ == "__main__":
    sys.exit(main())
