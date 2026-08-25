#!/usr/bin/env python3
"""Fail when a tracked documentation page declares no title and no rank.

A Docusaurus page with no front matter still renders. What it loses is the two
values the sidebar needs: the label it shows, which then falls back to the
first heading, and the rank it sorts on, which then falls back to the file
name. Ninety-six pages of the operator help were in that state, in both
locales, so the sidebar of a section was the alphabetical order of French
slugs, and one page carrying a rank jumped ahead of every page carrying none
whatever its value.

Neither fallback is a decision anybody made, and neither is visible from the
source: the reading order of a section only appears once the site is built.
This guard puts both values back in the file, where a reviewer sees them.

The subject is the git inventory, never the disk. `docs/site/docs/reference/api`
holds 401 generated files that git ignores, 123 of which carry no
`sidebar_position` and never should: a guard reading the disk would demand a
rank from them, or be born with a hand-written exclusion list. Reading
`git ls-files` removes both, and satisfies property 6 of
`scripts/check_selftest.py`, "a guard reads the same set of files whatever tree
it runs in".

What counts as a violation: a tracked page under either documentation root
whose front matter is absent, whose `title` is missing or blank, or whose
`sidebar_position` is missing or is not a number. A number, not an integer:
`plugin-content-docs/lib/frontMatter.js:29` validates the key with
`JoiFrontMatter.number()`, and four pages of `how-to` use `6.5` and `6.6` to
slot between two existing ranks. A guard demanding an integer would be red on a
tree Docusaurus is happy with, which is a rule inventing its own defect.

A rank must also be unique among the sibling pages of its directory. Two pages
of `how-to` shared rank 9 and three pages of `reference` shared rank 6, so the
reading order of five pages was the alphabetical order of their file names,
which is exactly the fallback the rank exists to replace, invisible from the
source while every page dutifully carried a number. Docusaurus does not warn:
at equal rank it silently falls back to the file name.

What this does not catch, and it is deliberate: whether the `title` matches the
first heading of the page. Measured over the 210 tracked pages, 17 legitimately
differ, 15 of them because the heading wraps the same text in backticks, and 3
more carry no heading at all. A rule born with 20 exemptions over 210 pages is
a list nobody re-reads, so the equality is checked once, by the change that
introduces it, and not by a standing guard.

Exit codes:
    0  every tracked page read, all carry a title and a unique rank
    1  at least one page is missing a title or a rank, or shares its rank
       with a sibling page
    2  nothing was measured, or a root that must be covered was not read

Usage:
    python3 scripts/check_docs_frontmatter.py
    python3 scripts/check_docs_frontmatter.py --selftest
"""

import argparse
import contextlib
import io
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]

# The two documentation roots, both locales. The French mirror is a full copy
# of the pages, and the build reads each page's front matter from the mirror
# when one exists, so a rank posted on the English side alone leaves the French
# sidebar in its fallback order. Both roots are therefore judged.
ROOTS = (
    Path("docs/site/docs"),
    Path("docs/site/i18n/fr/docusaurus-plugin-content-docs/current"),
)

PAGE_SUFFIXES = ("*.md", "*.mdx")

# Asserted positively, one page per section per locale, so that a `pathspec`
# that narrowed, or a mirror that moved, fails the run instead of quietly
# reporting green on a smaller set. A count alone would not do it: an inventory
# that found many files proves it walked, not that it walked here.
REQUIRED_COVERAGE = (
    Path("docs/site/docs/operator-help/index.md"),
    Path("docs/site/docs/operator-help/installation/installer-sur-macos.md"),
    Path("docs/site/docs/operator-help/troubleshooting/un-agent-est-bloque.md"),
    Path("docs/site/docs/explanation/the-8-principles.md"),
    Path(
        "docs/site/i18n/fr/docusaurus-plugin-content-docs/current"
        "/operator-help/index.md"
    ),
    Path(
        "docs/site/i18n/fr/docusaurus-plugin-content-docs/current"
        "/operator-help/installation/installer-sur-macos.md"
    ),
    Path(
        "docs/site/i18n/fr/docusaurus-plugin-content-docs/current"
        "/operator-help/troubleshooting/un-agent-est-bloque.md"
    ),
)

FRONT_MATTER = re.compile(r"\A---\r?\n(.*?)\r?\n---\r?\n", re.DOTALL)
SCALAR = re.compile(r"^([A-Za-z_][\w.-]*)\s*:\s*(.*?)\s*$")


def tracked_pages(root: Path) -> list[Path]:
    """Return the pages git tracks under `root`, as repository-relative paths.

    Raises `RuntimeError` when git itself fails, so that an inventory nobody
    could read reports nothing measured rather than an empty list, which would
    be indistinguishable from a root with no page in it.
    """
    pathspecs = [f"{root}/{suffix}" for suffix in PAGE_SUFFIXES]
    result = subprocess.run(
        ["git", "ls-files", "-z", "--", *pathspecs],
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
        check=False,
    )
    if result.returncode != 0:
        raise RuntimeError(
            f"`git ls-files` exited {result.returncode} on {root}: "
            f"{result.stderr.strip()!r}"
        )
    return sorted(Path(entry) for entry in result.stdout.split("\0") if entry)


def parse_front_matter(text: str) -> dict[str, str] | None:
    """Return the top-level scalars of the front matter, or None if absent.

    Nested blocks are skipped rather than parsed: the two keys this guard
    judges are scalars at the top level, and a hand-rolled reader that dived
    into a nested mapping would read a nested `title` as the page's own.
    """
    match = FRONT_MATTER.match(text)
    if match is None:
        return None
    scalars: dict[str, str] = {}
    for line in match.group(1).splitlines():
        if not line.strip() or line.lstrip().startswith("#"):
            continue
        if line[:1] in (" ", "\t", "-"):
            continue
        found = SCALAR.match(line)
        if found is None:
            continue
        key, raw = found.groups()
        if len(raw) >= 2 and raw[0] == raw[-1] and raw[0] in "\"'":
            raw = raw[1:-1]
        scalars[key] = raw.strip()
    return scalars


def page_faults(root_path: Path, pages: list[Path]) -> list[tuple[Path, str]]:
    """Return each page that misses a title or a rank, with what it misses.

    Pure with respect to the tree it is given, so a caller can drive it on a
    fixture that violates the rule as well as on the repository, which is the
    only way to know the detector fires at all.
    """
    faults: list[tuple[Path, str]] = []
    for page in pages:
        text = (root_path / page).read_text(encoding="utf-8", errors="replace")
        front = parse_front_matter(text)
        if front is None:
            faults.append((page, "no front matter"))
            continue
        title = front.get("title")
        if title is None:
            faults.append((page, "no `title`"))
        elif not title.strip():
            faults.append((page, "blank `title`"))
        rank = front.get("sidebar_position")
        if rank is None:
            faults.append((page, "no `sidebar_position`"))
        else:
            try:
                float(rank)
            except ValueError:
                faults.append((page, f"`sidebar_position` is not a number: {rank!r}"))
    return faults


def rank_faults(root_path: Path, pages: list[Path]) -> list[tuple[Path, str]]:
    """Return each page whose rank collides with a sibling page's rank.

    The grouping key is the page's own directory, which is the unit Docusaurus
    sorts: two equal ranks in one directory fall back to file-name order, and
    that order is visible nowhere in the source. Ranks are compared as
    numbers, so `6` and `6.0` collide. Pages whose rank is absent or not a
    number are `page_faults`'s subject and are skipped here, one fault per
    defect.

    Pure with respect to the tree it is given, for the same reason as
    `page_faults`.
    """
    by_dir: dict[tuple[Path, float], list[Path]] = {}
    for page in pages:
        text = (root_path / page).read_text(encoding="utf-8", errors="replace")
        front = parse_front_matter(text)
        if front is None:
            continue
        rank = front.get("sidebar_position")
        if rank is None:
            continue
        try:
            value = float(rank)
        except ValueError:
            continue
        by_dir.setdefault((page.parent, value), []).append(page)
    faults: list[tuple[Path, str]] = []
    for (_, value), sharers in sorted(by_dir.items()):
        if len(sharers) < 2:
            continue
        names = ", ".join(p.name for p in sharers)
        for page in sharers:
            faults.append(
                (
                    page,
                    f"`sidebar_position` {value:g} shared by {len(sharers)} "
                    f"sibling pages ({names}), so their order is the file-name "
                    f"fallback",
                )
            )
    return faults


def uncovered_required(inventory: set[Path]) -> list[Path]:
    """Return the anchor pages that exist on disk but fell outside the walk."""
    return [
        anchor
        for anchor in REQUIRED_COVERAGE
        if (REPO_ROOT / anchor).is_file() and anchor not in inventory
    ]


def report(roots: tuple[Path, ...] = ROOTS) -> int:
    per_root: dict[Path, list[Path]] = {}
    for root in roots:
        try:
            per_root[root] = tracked_pages(root)
        except RuntimeError as exc:
            print(f"check_docs_frontmatter: NO COVERAGE, {exc}", file=sys.stderr)
            return 2
        if not per_root[root]:
            print(
                f"check_docs_frontmatter: NO COVERAGE, git tracks no page under "
                f"{root}",
                file=sys.stderr,
            )
            return 2

    inventory = [page for pages in per_root.values() for page in pages]
    missing = uncovered_required(set(inventory))
    if missing:
        print(
            "check_docs_frontmatter: NO COVERAGE, these pages exist but were "
            "not read:",
            file=sys.stderr,
        )
        for anchor in missing:
            print(f"  {anchor}", file=sys.stderr)
        return 2

    faults = page_faults(REPO_ROOT, inventory)
    duplicates = rank_faults(REPO_ROOT, inventory)
    counts = {
        "no front matter": sum(1 for _, why in faults if why == "no front matter"),
        "title": sum(1 for _, why in faults if "`title`" in why),
        "sidebar_position": sum(
            1 for _, why in faults if "`sidebar_position`" in why
        ),
    }
    faults += duplicates
    breakdown = " + ".join(
        f"{len(pages)} under {root}" for root, pages in per_root.items()
    )
    print(
        f"check_docs_frontmatter: {len(inventory)} tracked pages read, "
        f"{breakdown}"
    )
    print(
        f"check_docs_frontmatter: {counts['no front matter']} with no front "
        f"matter, {counts['title']} with no usable `title`, "
        f"{counts['sidebar_position']} with no usable `sidebar_position`, "
        f"{len(duplicates)} sharing a rank with a sibling"
    )
    sys.stdout.flush()

    if faults:
        print(
            f"\n{len(faults)} defect(s). A page with no `title` shows whatever "
            f"its first heading happens to say in the sidebar, and a page with "
            f"no `sidebar_position` sorts on its file name, so the reading "
            f"order of its section is its slugs in alphabetical order:\n",
            file=sys.stderr,
        )
        for page, why in faults:
            print(f"  {page}", file=sys.stderr)
            print(f"    {why}", file=sys.stderr)
        print(
            "\nAdd a front matter carrying `title` and a `sidebar_position` no "
            "sibling page already uses (decimals such as 6.5 slot between two "
            "ranks). Both locales: the build reads the French mirror's own "
            "front matter when the page has one.",
            file=sys.stderr,
        )
        return 1

    print("check_docs_frontmatter: every tracked page declares its title and its rank")
    return 0


def _case(name: str, condition: bool) -> bool:
    print(f"  {'ok  ' if condition else 'FAIL'}  {name}")
    return condition


def _write(root: Path, relative: str, body: str) -> Path:
    path = root / relative
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(body, encoding="utf-8")
    return Path(relative)


COMPLETE = """\
---
title: Install Apollia on macOS
sidebar_position: 1
---

# Install Apollia on macOS

Body.
"""

NO_FRONT_MATTER = """\
# Install Apollia on macOS

Body.
"""

TITLE_ONLY = """\
---
title: Install Apollia on macOS
---

# Install Apollia on macOS
"""

DECIMAL_RANK = """\
---
title: Install the desktop app
sidebar_position: 6.5
---

# Install the desktop app
"""

TEXT_RANK = """\
---
title: Install the desktop app
sidebar_position: first
---

# Install the desktop app
"""

RANK_ONLY = """\
---
sidebar_position: 1
---

# Install Apollia on macOS
"""

BLANK_TITLE = """\
---
title:
sidebar_position: 1
---

# Install Apollia on macOS
"""

NESTED_TITLE = """\
---
sidebar_position: 1
sidebar_custom_props:
  title: not the page title
---

# Install Apollia on macOS
"""


def selftest() -> int:
    print("docs front matter: both directions on a built subject")
    root = Path(tempfile.mkdtemp(prefix="check-docs-frontmatter-"))
    try:

        def measure(body: str) -> list[tuple[Path, str]]:
            page = _write(root, "page.md", body)
            return page_faults(root, [page])

        results = [
            # The positive control. Without it, a detector that reported every
            # page as faulty would satisfy every negative case below while
            # being worthless.
            _case("a complete front matter raises nothing", not measure(COMPLETE)),
            _case(
                "a page with no front matter is named once",
                [why for _, why in measure(NO_FRONT_MATTER)] == ["no front matter"],
            ),
            _case(
                "a title without a rank names the rank",
                [why for _, why in measure(TITLE_ONLY)] == ["no `sidebar_position`"],
            ),
            _case(
                "a rank without a title names the title",
                [why for _, why in measure(RANK_ONLY)] == ["no `title`"],
            ),
            _case(
                "a `title:` with nothing after it is a defect, not a title",
                [why for _, why in measure(BLANK_TITLE)] == ["blank `title`"],
            ),
            _case(
                "a `title` nested under another key is not the page title",
                [why for _, why in measure(NESTED_TITLE)] == ["no `title`"],
            ),
            # The rank Docusaurus accepts is a number. Slotting a page between
            # two ranks is what `6.5` is for, and a guard red on it would be
            # inventing a rule the generator does not carry.
            _case(
                "a decimal rank is a rank, not a defect",
                not measure(DECIMAL_RANK),
            ),
            _case(
                "a rank that is not a number is named",
                [why for _, why in measure(TEXT_RANK)]
                == ["`sidebar_position` is not a number: 'first'"],
            ),
        ]

        # Rank uniqueness, both directions, on a multi-page fixture: the rule
        # groups by directory and compares numerically, and each of those two
        # choices is asserted so a rewrite cannot quietly drop one.
        def ranked(relative: str, rank: str) -> Path:
            return _write(
                root,
                relative,
                f"---\ntitle: A page\nsidebar_position: {rank}\n---\n\n# A page\n",
            )

        collide = [ranked("how-to/a.md", "6"), ranked("how-to/b.md", "6.0")]
        results.append(
            _case(
                "two sibling pages sharing a rank are both named, 6 == 6.0",
                len(rank_faults(root, collide)) == 2
                and all("shared by 2" in why for _, why in rank_faults(root, collide)),
            )
        )
        distinct = [ranked("how-to/a.md", "6"), ranked("how-to/b.md", "6.5")]
        results.append(
            _case(
                "distinct ranks in one directory raise nothing",
                not rank_faults(root, distinct),
            )
        )
        elsewhere = [ranked("how-to/a.md", "6"), ranked("reference/b.md", "6")]
        results.append(
            _case(
                "the same rank in two directories is not a collision",
                not rank_faults(root, elsewhere),
            )
        )

        # Nothing measured is not the same as nothing wrong. A root that
        # matches no tracked page must reach a distinct code, or a `pathspec`
        # that stopped matching would read as a pass.
        with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(
            io.StringIO()
        ):
            empty_verdict = report((Path("docs/site/docs/no-such-section"),))
        results.append(
            _case(
                "a root matching no tracked page reports nothing measured, not a pass",
                empty_verdict == 2,
            )
        )
        # Positive control on the same call: without it the code 2 above would
        # prove the root was empty only if `report` can reach 0 at all.
        with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(
            io.StringIO()
        ):
            real_verdict = report()
        results.append(
            _case(
                "positive control: the same entry point reaches a verdict on the tree",
                real_verdict in (0, 1),
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
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument(
        "--selftest", action="store_true", help="replay the fixture controls instead of measuring the tree"
    )
    if parser.parse_args().selftest:
        sys.exit(selftest())
    sys.exit(report())


if __name__ == "__main__":
    main()
