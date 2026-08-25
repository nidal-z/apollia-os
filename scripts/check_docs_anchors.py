#!/usr/bin/env python3
"""Resolve every internal fragment link of the built documentation site.

Docusaurus throws on a link to a missing page (`onBrokenLinks: 'throw'`) but
only warns on a link to a missing anchor: `onBrokenAnchors` defaults to
`warn` (`@docusaurus/core/lib/server/configValidation.js`) and
`docusaurus.config.js` does not override it. A build at exit 0 therefore says
nothing about fragments, and the decisions page is all fragments: every rule
of the corpus links to `/architecture/decisions#some-anchor`, so a renamed
anchor silently strands every link that named it.

This reads the rendered HTML, `<build>/**/*.html`, collects the ids each page
renders, and checks every internal `href="...#fragment"` against them. The
subject is the built site, not the sources, so it judges what a reader's
browser resolves, after MDX, plugins and slug rewriting have all had their
say.

The build directory is a product of `npm run build`, not of the checkout, so
its absence is a coverage hole, never a pass. Run this after the build; the
`docs-build` CI job does exactly that.

Exit codes:
    0  every fragment link of the built site resolves
    1  at least one link names a page or an anchor that does not exist
    2  nothing measured: the build directory is absent or holds no page

Usage:
    python3 scripts/check_docs_anchors.py [build_dir]
    python3 scripts/check_docs_anchors.py --selftest
"""

import argparse
import re
import shutil
import sys
import tempfile
from collections import defaultdict
from html.parser import HTMLParser
from pathlib import Path
from urllib.parse import unquote, urlsplit

REPO_ROOT = Path(__file__).resolve().parents[1]
DEFAULT_BUILD = REPO_ROOT / "docs/site/build"


class PageParser(HTMLParser):
    def __init__(self):
        super().__init__()
        self.ids = set()
        self.links = []

    def handle_starttag(self, tag, attrs):
        a = dict(attrs)
        if "id" in a:
            self.ids.add(a["id"])
        if tag == "a" and a.get("name"):
            self.ids.add(a["name"])
        if tag == "a" and a.get("href"):
            self.links.append(a["href"])


def route_of(html: Path, build: Path) -> str:
    rel = html.relative_to(build).as_posix()
    rel = re.sub(r"(^|/)index\.html$", "", rel)
    return "/" + rel.rstrip("/") + ("/" if rel else "")


def resolve(build: Path) -> tuple[int, int, dict] | None:
    """Return (pages, links checked, unresolved) or None when nothing was read."""
    if not build.is_dir():
        return None
    pages = {}
    for html in sorted(build.rglob("*.html")):
        parser = PageParser()
        parser.feed(html.read_text(encoding="utf-8", errors="replace"))
        pages[route_of(html, build)] = parser
    if not pages:
        return None
    ids = {route: parser.ids for route, parser in pages.items()}
    missing = defaultdict(list)
    checked = 0
    for route, parser in pages.items():
        for href in parser.links:
            u = urlsplit(href)
            if u.scheme or u.netloc or not u.fragment:
                continue
            target = u.path or route
            if not target.startswith("/"):
                target = route.rsplit("/", 1)[0] + "/" + target
            target = re.sub(r"/+", "/", target)
            if not target.endswith("/"):
                target += "/"
            checked += 1
            frag = unquote(u.fragment)
            if target not in ids:
                missing[(target, frag, "page")].append(route)
            elif frag not in ids[target]:
                missing[(target, frag, "anchor")].append(route)
    return len(pages), checked, missing


def report(build: Path = DEFAULT_BUILD) -> int:
    resolved = resolve(build)
    if resolved is None:
        print(
            f"check_docs_anchors: NO COVERAGE, {build} is absent or holds no "
            f"page. Build the site first: cd docs/site && npm run build",
            file=sys.stderr,
        )
        return 2
    n_pages, checked, missing = resolved
    print(
        f"check_docs_anchors: {n_pages} pages, {checked} fragment links "
        f"checked, {len(missing)} unresolved"
    )
    sys.stdout.flush()
    if missing:
        for (target, frag, kind), sources in sorted(missing.items()):
            sources = sorted(set(sources))
            print(
                f"  MISSING {kind}: {target}#{frag}  from {len(sources)} "
                f"page(s): {', '.join(sources[:3])}",
                file=sys.stderr,
            )
        print(
            "\nA fragment that resolves to nothing scrolls the reader to the "
            "top of the page and says nothing. The build only warns on these "
            "(`onBrokenAnchors` defaults to warn), which is why this guard "
            "exists.",
            file=sys.stderr,
        )
        return 1
    return 0


def _case(name: str, condition: bool) -> bool:
    print(f"  {'ok  ' if condition else 'FAIL'}  {name}")
    return condition


def selftest() -> int:
    print("docs anchors: both directions on a built subject")
    root = Path(tempfile.mkdtemp(prefix="check-docs-anchors-"))
    try:
        build = root / "build"
        (build / "a").mkdir(parents=True)
        (build / "b").mkdir(parents=True)
        (build / "a/index.html").write_text(
            '<html><body><h2 id="here">Here</h2>'
            '<a href="/b/#kept">fine</a></body></html>',
            encoding="utf-8",
        )
        (build / "b/index.html").write_text(
            '<html><body><h2 id="kept">Kept</h2>'
            '<a href="/a/#here">fine</a></body></html>',
            encoding="utf-8",
        )

        results = [
            # The positive control. Without it, a resolver that reported every
            # link as missing would satisfy the negative cases while being
            # worthless.
            _case("fragments that resolve pass", report(build) == 0),
        ]

        (build / "a/index.html").write_text(
            '<html><body><h2 id="here">Here</h2>'
            '<a href="/b/#gone">to a missing anchor</a>'
            '<a href="/nope/#x">to a missing page</a></body></html>',
            encoding="utf-8",
        )
        results.append(
            _case("a missing anchor and a missing page are both reported",
                  report(build) == 1)
        )

        results.append(
            _case(
                "an absent build directory reports nothing measured, not a pass",
                report(root / "no-such-build") == 2,
            )
        )

        print()
        if all(results):
            print(f"self-test: all {len(results)} cases pass")
            return 0
        print(f"self-test: {results.count(False)} of {len(results)} cases fail")
        return 1
    finally:
        shutil.rmtree(root, ignore_errors=True)


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Resolve every internal fragment link of the built docs site."
    )
    parser.add_argument(
        "build_dir",
        nargs="?",
        default=str(DEFAULT_BUILD),
        help="the Docusaurus build directory (default: docs/site/build)",
    )
    parser.add_argument(
        "--selftest",
        action="store_true",
        help="replay a positive and a negative control on a fixture, never on the tree",
    )
    args = parser.parse_args()
    if args.selftest:
        sys.exit(selftest())
    sys.exit(report(Path(args.build_dir)))


if __name__ == "__main__":
    main()
