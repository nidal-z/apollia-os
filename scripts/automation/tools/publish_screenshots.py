"""Copy an automaton run's captures into the documentation's image directory.

The automaton writes `NNN-<label>.png` into `.apollia-automation/`; the pages
reference `/img/operator-help/<label>.png`. Bridging the two was a manual copy,
which is how the set went stale without anyone noticing.

One directory, no locale. Both locales reference the same image and the
interface in it is English. The site used to keep `en/` and `fr/`, both filled
from one capture set: they stayed byte identical until the day only one was
refreshed, and then the English pages served French captures for two weeks with
no gate able to see it, because a stale image is not a broken link.

Usage:

  python3 scripts/automation/tools/publish_screenshots.py
  python3 scripts/automation/tools/publish_screenshots.py --apply
  python3 scripts/automation/tools/publish_screenshots.py --from DIR --apply

Without --apply it reports what it would do, which labels the run produced that
no page wants, and which the pages want that the run did not produce.
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
REF = re.compile(r"/img/operator-help/([a-z0-9-]+\.png)")


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

    # One directory, no locale dimension. The site used to keep en/ and fr/,
    # both filled from the same English capture set, and the two stayed byte
    # identical until the day only one was refreshed. Then the English pages
    # served French captures for two weeks and no gate could see it, because a
    # stale image is not a broken link. A single directory removes the failure
    # mode rather than documenting it.
    IMG.mkdir(parents=True, exist_ok=True)
    for name in published:
        if args.apply:
            shutil.copy2(produced[name], IMG / name)
    verb = "published" if args.apply else "would publish"
    print(f"{verb} into {IMG.relative_to(REPO_ROOT)}: {len(published)}")

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
