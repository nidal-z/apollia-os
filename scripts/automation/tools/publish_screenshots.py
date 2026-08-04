#!/usr/bin/env python3
"""Publish automation captures into the documentation image set.

The runner writes `NNN-<label>.png` into its output directory, where `NNN` is the
step sequence. The documentation references `<label>.png` under
`/img/operator-help/`. Bridging the two was a manual copy, which is how the set
drifted: an image nobody could regenerate, a capture nobody published, and no way
to tell which was which.

This does the copy and, more usefully, answers three questions the manual step
never did:

  - which documented images the run produced (published),
  - which it did not (missing, so still stale on the site),
  - which it produced that no page uses (extra, so dead weight).

It refuses to invent: a label the documentation does not reference is never
copied, because an unreferenced image in `static/` is a file that ships to every
visitor and is served to no one.

Usage:
    python3 scripts/automation/tools/publish_screenshots.py --locale both
    python3 scripts/automation/tools/publish_screenshots.py --locale both --apply
    python3 scripts/automation/tools/publish_screenshots.py --locale fr --from DIR

`--locale both` is the normal case and the one `SCREENSHOTS.md` prescribes: one
English capture set, copied into both image directories. The French pages
reference `/img/operator-help/fr/`, so the directory has to exist and be filled,
but the images in it are the English ones. Shooting the interface twice, once
per language, doubled a day's work to produce a second set nobody compared
against the first, and left the two halves of the site drifting apart whenever
only one pass was redone.

`--locale en` and `--locale fr` remain, for the day a locale genuinely diverges.

The default source is `$APOLLIA_AUTOMATION_OUT`, falling back to the runner's own
default under the system temp directory.
"""

import argparse
import os
import re
import shutil
import sys
import tempfile
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[3]
DOCS = REPO_ROOT / "docs" / "site" / "docs" / "operator-help"
MIRROR = (
    REPO_ROOT
    / "docs"
    / "site"
    / "i18n"
    / "fr"
    / "docusaurus-plugin-content-docs"
    / "current"
    / "operator-help"
)
IMG = REPO_ROOT / "docs" / "site" / "static" / "img" / "operator-help"

SEQ = re.compile(r"^\d{3}-(.+\.png)$")
REF = re.compile(r"/img/operator-help/(?:en|fr)/([a-z0-9-]+\.png)")


def referenced() -> set[str]:
    """Every image the published pages point at, both locales."""
    names: set[str] = set()
    for root in (DOCS, MIRROR):
        if not root.is_dir():
            continue
        for page in root.rglob("*.md"):
            names.update(REF.findall(page.read_text(encoding="utf-8")))
    return names


def default_source() -> Path:
    env = os.environ.get("APOLLIA_AUTOMATION_OUT")
    if env:
        return Path(env)
    return Path(tempfile.gettempdir()) / "apollia-automation"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--from", dest="source", type=Path, default=None)
    parser.add_argument(
        "--locale",
        required=True,
        choices=("en", "fr", "both"),
        help="which image directory to fill. 'both' publishes one English "
        "capture set into en/ and fr/, which is what the site actually needs: "
        "the French pages reference fr/ and there is only one set of images",
    )
    parser.add_argument("--apply", action="store_true", help="copy, rather than report")
    args = parser.parse_args()

    source = args.source or default_source()
    if not source.is_dir():
        print(f"error: no capture directory at {source}", file=sys.stderr)
        print("Run the automaton first, or pass --from.", file=sys.stderr)
        return 1

    wanted = referenced()
    if not wanted:
        print("error: no page references any image; refusing to guess", file=sys.stderr)
        return 1

    produced: dict[str, Path] = {}
    for path in sorted(source.glob("*.png")):
        match = SEQ.match(path.name)
        # A later step wins: re-capturing the same label in one run is how a
        # script fixes a screen it reached in a bad state.
        produced[match.group(1) if match else path.name] = path

    published = sorted(wanted & produced.keys())
    missing = sorted(wanted - produced.keys())
    extra = sorted(produced.keys() - wanted)

    locales = ("en", "fr") if args.locale == "both" else (args.locale,)
    for locale in locales:
        dest = IMG / locale
        dest.mkdir(parents=True, exist_ok=True)
        for name in published:
            if args.apply:
                shutil.copy2(produced[name], dest / name)
    verb = "published" if args.apply else "would publish"
    print(f"{verb} into {'/, '.join(locales)}/: {len(published)}")

    if missing:
        print(f"\nmissing from this run ({len(missing)}), still stale on the site:")
        for name in missing:
            print(f"  {name}")
    if extra:
        print(f"\ncaptured but referenced by no page ({len(extra)}), not copied:")
        for name in extra:
            print(f"  {name}")

    if not args.apply:
        print("\nDry run. Pass --apply to copy.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
