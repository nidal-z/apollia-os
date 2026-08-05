#!/usr/bin/env python3
"""Check that the shooting script and the published pages name the same images.

`scripts/automation/SCREENSHOTS.md` drives a whole day of manual work: one row
per image, and the File column is the name the operator saves under. The site
references those names from its Markdown. Nothing joins the two, and the failure
is silent in both directions:

  - a row whose File cell no page references produces an image that ships to
    every visitor and is served to no one,
  - a page whose image no row produces keeps whatever stale file is already
    there, with no error anywhere.

Neither shows up in a build. So this compares the two sets and fails on any
difference. It also checks the rows are numbered without a hole and that no file
name is claimed twice, because a duplicated name means the second shot silently
overwrites the first.

Exit codes: 0 the two sets agree, 1 they do not.
"""

import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
SCRIPT = REPO_ROOT / "scripts" / "automation" / "SCREENSHOTS.md"
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

# Same expression `publish_screenshots.py` uses to find image references, so the
# two tools cannot disagree about what counts as referenced.
# One directory, no locale segment. Both locales reference the same image:
# keeping en/ and fr/ side by side, filled from one capture set, is what let
# the English pages serve French captures for two weeks without any gate
# noticing, since a stale image is not a broken link.
REF = re.compile(r"/img/operator-help/([a-z0-9-]+\.png)")

# A shooting row: | 12 | `some-name.png` | ... The row number and the File cell
# are the only two columns this cares about.
ROW = re.compile(r"^\|\s*(\d+)\s*\|\s*`([a-z0-9-]+\.png)`\s*\|")


def short(path: Path) -> str:
    """Repo-relative path when it is inside the repo, absolute otherwise."""
    try:
        return str(path.relative_to(REPO_ROOT))
    except ValueError:
        return str(path)


def rows_from_script(text: str) -> list[tuple[int, str]]:
    """Every (row number, file name) pair the shooting script declares."""
    out: list[tuple[int, str]] = []
    for line in text.splitlines():
        m = ROW.match(line.strip())
        if m:
            out.append((int(m.group(1)), m.group(2)))
    return out


def referenced_by_pages() -> set[str]:
    """Every image the published pages point at, both locales."""
    names: set[str] = set()
    for root in (DOCS, MIRROR):
        if not root.is_dir():
            continue
        for page in root.rglob("*.md"):
            names.update(REF.findall(page.read_text(encoding="utf-8")))
    return names


def check() -> list[str]:
    """Return the problems found. Empty means the two sets agree."""
    problems: list[str] = []

    if not SCRIPT.is_file():
        return [f"{short(SCRIPT)} does not exist"]

    rows = rows_from_script(SCRIPT.read_text(encoding="utf-8"))
    if not rows:
        return [f"{short(SCRIPT)} declares no shooting row at all"]

    numbers = [n for n, _ in rows]
    expected = list(range(1, len(numbers) + 1))
    if numbers != expected:
        gaps = sorted(set(expected) - set(numbers))
        dupes = sorted({n for n in numbers if numbers.count(n) > 1})
        detail = []
        if gaps:
            detail.append(f"missing {gaps}")
        if dupes:
            detail.append(f"repeated {dupes}")
        problems.append(
            "row numbers do not run 1..N without a hole: " + ", ".join(detail or ["out of order"])
        )

    names = [f for _, f in rows]
    for name in sorted({n for n in names if names.count(n) > 1}):
        problems.append(f"file name claimed by more than one row, the second shot wins: {name}")

    declared = set(names)
    used = referenced_by_pages()

    for name in sorted(declared - used):
        problems.append(f"shot but no page references it, it would ship unused: {name}")
    for name in sorted(used - declared):
        problems.append(f"referenced by a page but no row shoots it, it stays stale: {name}")

    return problems


def self_test() -> int:
    """Prove this check can fail, on each mismatch it claims to catch.

    A verifier whose negative case nobody exercised has, on this repository,
    repeatedly turned out to pass on anything. So each failure mode is replayed
    here against a synthetic pair of trees, and the last case replays a
    consistent pair to prove the check is not simply always red.
    """
    import tempfile

    global SCRIPT, DOCS, MIRROR
    saved = (SCRIPT, DOCS, MIRROR)
    failures: list[str] = []

    def run_case(name: str, script_body: str, page_body: str, must_mention: str | None) -> None:
        global SCRIPT, DOCS, MIRROR
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "docs").mkdir()
            SCRIPT = root / "SCREENSHOTS.md"
            DOCS = root / "docs"
            MIRROR = root / "absent"
            SCRIPT.write_text(script_body, encoding="utf-8")
            (DOCS / "page.md").write_text(page_body, encoding="utf-8")
            problems = check()
            joined = " | ".join(problems)
            if must_mention is None:
                if problems:
                    failures.append(f"{name}: expected no problem, got: {joined}")
            elif not any(must_mention in p for p in problems):
                failures.append(f"{name}: expected a problem mentioning '{must_mention}', got: {joined or 'none'}")

    header = "| # | File |\n|---|---|\n"
    row_a = "| 1 | `alpha.png` |\n"
    row_b = "| 2 | `beta.png` |\n"
    ref_a = "![a](/img/operator-help/alpha.png)\n"
    ref_b = "![b](/img/operator-help/beta.png)\n"

    run_case("consistent pair", header + row_a + row_b, ref_a + ref_b, None)
    run_case("row shoots an image no page uses", header + row_a + row_b, ref_a, "beta.png")
    run_case("page wants an image no row shoots", header + row_a, ref_a + ref_b, "beta.png")
    run_case(
        "two rows claim one name",
        header + row_a + "| 2 | `alpha.png` |\n",
        ref_a,
        "more than one row",
    )
    run_case(
        "row numbering has a hole",
        header + row_a + "| 3 | `beta.png` |\n",
        ref_a + ref_b,
        "do not run 1..N",
    )
    run_case("script declares nothing", header, ref_a, "no shooting row")

    SCRIPT, DOCS, MIRROR = saved

    if failures:
        print(f"{len(failures)} self-test case(s) failed:")
        for f in failures:
            print(f"  - {f}")
        return 1
    print("6 self-test cases passed, the check fails on each mismatch it claims to catch")
    return 0


def counts_in_prose_match_the_scripts() -> list[str]:
    """The shooting doctrine quotes three numbers. Prose drifts, scripts do not.

    SCREENSHOTS.md tells the maintainer to expect N captures from the
    deterministic script, M from the model one, and states N+M of 85 as
    automated coverage. Those three were written by hand and were already wrong
    once, by six, the day six captures were added. A wrong count here is not
    cosmetic: it is how someone concludes a run failed when it did exactly what
    it was asked.
    """
    import json as _json

    problems: list[str] = []
    prose = SCRIPT.read_text(encoding="utf-8")
    totals = {}
    for name in ("screenshots-en", "screenshots-en-llm"):
        path = REPO_ROOT / "scripts" / "automation" / f"{name}.json"
        steps = _json.loads(path.read_text(encoding="utf-8"))["steps"]
        totals[name] = sum(1 for s in steps if s.get("kind") == "screenshot")

    for name, n in totals.items():
        if f"{n} captures" not in prose:
            problems.append(
                f"{SCRIPT.name} does not say '{n} captures' anywhere, but {name}.json "
                f"carries {n} screenshot steps"
            )
    total = sum(totals.values())
    if f"**{total} of the 85" not in prose:
        problems.append(
            f"{SCRIPT.name} does not claim '{total} of the 85' as automated coverage, "
            f"which is what the two scripts add up to"
        )
    return problems


def main() -> int:
    if "--self-test" in sys.argv[1:]:
        return self_test()

    problems = check() + counts_in_prose_match_the_scripts()
    if problems:
        print(f"{len(problems)} problem(s) between SCREENSHOTS.md and the published pages:")
        for p in problems:
            print(f"  - {p}")
        return 1

    rows = rows_from_script(SCRIPT.read_text(encoding="utf-8"))
    print(f"{len(rows)} shooting rows, {len(referenced_by_pages())} referenced images, sets agree")
    return 0


if __name__ == "__main__":
    sys.exit(main())
