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

# The two trees a user actually loads: the desktop webview bundle and the
# documentation site. Whole trees, not a list of interesting subdirectories.
#
# The first version of this file listed five subdirectories plus two named
# files, and missed four places a remote font fits: `ui/overlay.html` (the
# second Vite entry point, built and loaded by the webview alongside
# index.html), `ui/vite.config.ts` and `ui/tailwind.config.ts` (both of which
# can inject a stylesheet link), and `ui/public/` (served verbatim). A gate
# whose coverage is a hand-maintained list drifts the moment someone adds an
# entry point, and it drifts silently, in the direction that reports success.
# Rooting at the tree removes the class instead of the four instances.
SCAN_ROOTS = [
    Path("docs/site"),
    Path("crates/apollia-desktop/ui"),
]

# Entry points that must be inside the scanned set for the gate to mean
# anything. Asserted by the self-test, so shrinking SCAN_ROOTS back to a
# subdirectory list fails the build rather than quietly narrowing coverage.
REQUIRED_COVERAGE = [
    Path("docs/site/docusaurus.config.js"),
    Path("docs/site/src/css/custom.css"),
    Path("crates/apollia-desktop/ui/index.html"),
    Path("crates/apollia-desktop/ui/overlay.html"),
    Path("crates/apollia-desktop/ui/vite.config.ts"),
    Path("crates/apollia-desktop/ui/tailwind.config.ts"),
    Path("crates/apollia-desktop/ui/src/app.css"),
]

SCAN_SUFFIXES = {".css", ".scss", ".html", ".js", ".jsx", ".ts", ".tsx", ".svelte", ".md", ".mdx"}

# Derived output only. The build directory is what this rule protects, not what
# it inspects, and `node_modules` holds the bundled `@fontsource` packages whose
# own upstream sources legitimately name the CDN they were vendored from.
EXCLUDED_DIRS = {
    "node_modules",
    "build",
    ".docusaurus",
    "dist",
    ".git",
    ".svelte-kit",
    "coverage",
    "playwright-report",
    "test-results",
}

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
    return sorted(set(files))


def uncovered_required() -> list[Path]:
    """Return the entry points that exist on disk but fall outside the scan.

    Exists so the self-test can assert coverage positively. `iter_files`
    returning a large number proves the walk found files, not that it found the
    ones that matter: `index.html` can carry the stylesheet link that
    `src/` never will.
    """
    scanned = set(iter_files())
    missing = []
    for rel in REQUIRED_COVERAGE:
        path = REPO_ROOT / rel
        if path.is_file() and path not in scanned:
            missing.append(rel)
    return missing


def main() -> int:
    files = iter_files()
    if not files:
        # A rule that examined nothing is not a rule that holds.
        print("check_no_font_cdn: NO COVERAGE, no source file matched", file=sys.stderr)
        return 1

    missing = uncovered_required()
    if missing:
        print(
            "check_no_font_cdn: NO COVERAGE, these entry points exist but are "
            "outside the scanned roots:",
            file=sys.stderr,
        )
        for rel in missing:
            print(f"  {rel}", file=sys.stderr)
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
