#!/usr/bin/env python3
"""Fail when a French mirror page drifts in structure from its English source.

The English page of `operator-help/installation/configurer-votre-profil.md`
promised "blocks all network egress" while its French mirror had been rewritten
to say the opposite, with a 13-line caution box the English page did not carry.
The fix had landed on one locale only, and nothing compared the two: the claim
guards compare claim markers, and that sentence carried none.

Prose cannot be diffed across languages, but structure can. For each page that
exists in both locales this guard counts H2 and H3 headings, fenced code
blocks, images, internal links, `apollia-os` citations and admonitions. A
mirror whose counts differ from the canonical page has drifted in structure,
whatever its prose says. A same-shaped error present in both locales stays out
of reach, and that is the known limit of the method, not a bug of this guard.

The guard also owns the pairing rule itself, because "which pages must have a
mirror" was written nowhere and 18 generated pages quietly had none:

  - every English page must have a French mirror, except the generated
    reference pages named in `EN_ONLY_GENERATED`. Those are machine output
    that `docs/site/regen.sh` writes on the English side only; a mirror would
    be English text served under `/fr/`, which "one language per file"
    (`docs/agents/DOCS-WRITING.md` section 4) forbids.
  - an exempted page that grows a mirror anyway is reported too: somebody
    started translating machine output, so either the generator changed or
    the exemption line has become a lie.
  - every French page must have an English source. An orphan mirror is a page
    the canonical locale cannot see.

The inventory is `git ls-files`, never the disk, so the guard reads the same
set of files whatever tree it runs in (`docs/site/docs/reference/api` holds
hundreds of generated files git ignores).

Exit codes:
    0  every pair agrees and the pairing rule holds
    1  at least one drifted pair, missing mirror, stale exemption or orphan
    2  nothing measured: no tracked page, or no pair to compare

Usage:
    python3 scripts/check_docs_mirror.py
    python3 scripts/check_docs_mirror.py --selftest
"""

import argparse
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]

EN_ROOT = Path("docs/site/docs")
FR_ROOT = Path("docs/site/i18n/fr/docusaurus-plugin-content-docs/current")

# Generated reference pages, English-only by decision (DOCS-WRITING.md
# section 4). Prefixes, relative to the English root, one per generator
# output directory.
EN_ONLY_GENERATED = (
    "reference/cli/",
    "reference/sdk/",
)

PAGE_SUFFIXES = (".md", ".mdx")

FRONT = re.compile(r"\A---\n.*?\n---\n", re.S)
FENCE_BLOCK = re.compile(r"```.*?```", re.S)
FENCE_LINE = re.compile(r"^```", re.M)


def tracked_pages(repo: Path, root: Path) -> list[Path]:
    """Return the tracked pages under `root`, relative to `root`."""
    result = subprocess.run(
        ["git", "ls-files", "-z", "--", str(root)],
        cwd=repo,
        capture_output=True,
        text=True,
        check=False,
    )
    if result.returncode != 0:
        raise RuntimeError(
            f"`git ls-files` exited {result.returncode} on {root}: "
            f"{result.stderr.strip()!r}"
        )
    pages = []
    for entry in result.stdout.split("\0"):
        if entry and entry.endswith(PAGE_SUFFIXES):
            pages.append(Path(entry).relative_to(root))
    return sorted(pages)


def measure(path: Path) -> dict[str, int]:
    """Count the structural features of one page."""
    text = FRONT.sub("", path.read_text(encoding="utf-8", errors="replace"))
    nofence = FENCE_BLOCK.sub("", text)
    return {
        "h2": len(re.findall(r"^## ", nofence, re.M)),
        "h3": len(re.findall(r"^### ", nofence, re.M)),
        "fences": len(FENCE_LINE.findall(text)) // 2,
        "images": len(re.findall(r"!\[[^\]]*\]\(", nofence)),
        "links": len(re.findall(r"(?<!!)\[[^\]]*\]\((?!http)[^)]+\)", nofence)),
        "cli": len(re.findall(r"apollia-os ", text)),
        "admonitions": len(re.findall(r"^:::", nofence, re.M)),
    }


def pairing_faults(
    en_pages: list[Path],
    fr_pages: list[Path],
    en_only: tuple[str, ...] = EN_ONLY_GENERATED,
) -> list[tuple[Path, str]]:
    """Return every page that breaks the pairing rule, with the rule broken.

    Pure with respect to the inventories it is given, so the selftest can
    drive it on a fixture, which is the only way to know it fires at all.
    """
    faults: list[tuple[Path, str]] = []
    fr_set = set(fr_pages)
    en_set = set(en_pages)
    for page in en_pages:
        posix = page.as_posix()
        exempt = any(posix.startswith(prefix) for prefix in en_only)
        if page not in fr_set and not exempt:
            faults.append((page, "no French mirror, and not a generated page"))
        elif page in fr_set and exempt:
            faults.append(
                (
                    page,
                    "exempted as generated English-only, yet a French mirror "
                    "exists, so the exemption line is now false",
                )
            )
    for page in fr_pages:
        if page not in en_set:
            faults.append((page, "French page with no English source, orphan mirror"))
    return faults


def report(
    repo: Path = REPO_ROOT,
    en_root: Path = EN_ROOT,
    fr_root: Path = FR_ROOT,
    en_only: tuple[str, ...] = EN_ONLY_GENERATED,
) -> int:
    try:
        en_pages = tracked_pages(repo, en_root)
        fr_pages = tracked_pages(repo, fr_root)
    except RuntimeError as exc:
        print(f"check_docs_mirror: NO COVERAGE, {exc}", file=sys.stderr)
        return 2
    if not en_pages or not fr_pages:
        print(
            f"check_docs_mirror: NO COVERAGE, git tracks {len(en_pages)} page(s) "
            f"under {en_root} and {len(fr_pages)} under {fr_root}",
            file=sys.stderr,
        )
        return 2

    faults = pairing_faults(en_pages, fr_pages, en_only)

    pairs = 0
    drift: list[tuple[Path, dict]] = []
    for page in en_pages:
        fr_path = repo / fr_root / page
        if not fr_path.is_file():
            continue
        pairs += 1
        a = measure(repo / en_root / page)
        b = measure(fr_path)
        diff = {k: (a[k], b[k]) for k in a if a[k] != b[k]}
        if diff:
            drift.append((page, diff))

    if pairs == 0:
        print(
            "check_docs_mirror: NO COVERAGE, no page exists in both locales",
            file=sys.stderr,
        )
        return 2

    exempt_count = sum(
        1
        for page in en_pages
        if any(page.as_posix().startswith(p) for p in en_only)
    )
    print(
        f"check_docs_mirror: {pairs} pairs compared, {len(drift)} with a "
        f"structural difference, {exempt_count} generated English-only "
        f"page(s) exempt from mirroring"
    )
    sys.stdout.flush()

    if drift or faults:
        for page, diff in drift:
            detail = ", ".join(f"{k} en={v[0]} fr={v[1]}" for k, v in diff.items())
            print(f"  DRIFT {page.as_posix()}: {detail}", file=sys.stderr)
        for page, why in faults:
            print(f"  PAIRING {page.as_posix()}: {why}", file=sys.stderr)
        print(
            "\nA mirror that drifted says something different in one language. "
            "Align the two pages; if the divergence is intentional, it is not a "
            "mirror any more and the pairing rule above must change with it.",
            file=sys.stderr,
        )
        return 1

    print("check_docs_mirror: every mirrored pair agrees in structure")
    return 0


def _case(name: str, condition: bool) -> bool:
    print(f"  {'ok  ' if condition else 'FAIL'}  {name}")
    return condition


PAGE = """\
---
title: A page
sidebar_position: 1
---

# A page

## First section

Some prose with a [link](/other/page) and `apollia-os agent list`.

:::note

An admonition.

:::
"""


def _fixture(root: Path) -> tuple[Path, Path]:
    en = root / "docs/site/docs"
    fr = root / "docs/site/i18n/fr/docusaurus-plugin-content-docs/current"
    for base in (en, fr):
        (base / "reference/sdk").mkdir(parents=True)
    (en / "page.md").write_text(PAGE, encoding="utf-8")
    (fr / "page.md").write_text(PAGE, encoding="utf-8")
    (en / "reference/sdk/llm.md").write_text(PAGE, encoding="utf-8")
    subprocess.run(["git", "init", "-q"], cwd=root, check=True)
    subprocess.run(["git", "add", "docs"], cwd=root, check=True)
    return en, fr


def selftest() -> int:
    print("docs mirror: both directions on a built subject")
    root = Path(tempfile.mkdtemp(prefix="check-docs-mirror-"))
    try:
        en, fr = _fixture(root)
        en_rel = Path("docs/site/docs")
        fr_rel = Path("docs/site/i18n/fr/docusaurus-plugin-content-docs/current")

        def run() -> int:
            return report(root, en_rel, fr_rel, ("reference/sdk/",))

        results = [
            # The positive control. Without it, a detector red on every pair
            # would satisfy the drift case below while being worthless.
            _case("identical mirrors and a listed generated page pass", run() == 0),
        ]

        # A drifted admonition, the exact shape of the defect that motivated
        # this guard: one locale carries a caution box the other does not.
        (fr / "page.md").write_text(
            PAGE + "\n:::caution\n\nOne locale only.\n\n:::\n", encoding="utf-8"
        )
        results.append(_case("an admonition present in one locale is a drift", run() == 1))
        (fr / "page.md").write_text(PAGE, encoding="utf-8")

        # The pairing rule, all three directions.
        (en / "orphan.md").write_text(PAGE, encoding="utf-8")
        subprocess.run(["git", "add", "docs"], cwd=root, check=True)
        results.append(
            _case("an English page with no mirror outside the list is a fault", run() == 1)
        )
        (en / "orphan.md").unlink()

        (fr / "reference/sdk/llm.md").write_text(PAGE, encoding="utf-8")
        subprocess.run(["git", "add", "docs"], cwd=root, check=True)
        results.append(
            _case("a mirror under an exempted prefix is a stale exemption", run() == 1)
        )
        (fr / "reference/sdk/llm.md").unlink()

        (fr / "orphan-fr.md").write_text(PAGE, encoding="utf-8")
        subprocess.run(["git", "add", "docs"], cwd=root, check=True)
        results.append(_case("a French page with no English source is a fault", run() == 1))
        (fr / "orphan-fr.md").unlink()
        subprocess.run(["git", "add", "docs"], cwd=root, check=True)

        # Nothing measured is not the same as nothing wrong.
        empty = Path("docs/site/docs/no-such-dir")
        results.append(
            _case(
                "a root matching no tracked page reports nothing measured",
                report(root, empty, fr_rel, ()) == 2,
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
        description="Compare the structure of every English page with its French mirror."
    )
    parser.add_argument(
        "--selftest",
        action="store_true",
        help="replay a positive and a negative control on a fixture, never on the tree",
    )
    args = parser.parse_args()
    if args.selftest:
        sys.exit(selftest())
    sys.exit(report())


if __name__ == "__main__":
    main()
