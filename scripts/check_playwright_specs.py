#!/usr/bin/env python3
"""Every Playwright spec is reachable by the runner and anchored in the UI.

Two silent failure modes motivated this guard, both found on the same tree:

  - `playwright.config.ts` declared `testDir: "./tests/perf"`, so 5 of the 7
    specs under `tests/` were invisible to the runner. `npx playwright test
    tests/settings --list` answered "Total: 0 tests in 0 files", which is not
    an error to any caller: 565 lines of test code could neither pass nor fail.
  - 5 of the 48 `data-testid` anchors used by those specs had been removed
    from `src/` by later commits. A renamed or deleted testid does not fail a
    spec at rest; it fails, or silently skips, only at runtime, which nothing
    reached.

The corpus of resolvable testids is the one the automation validator already
computes (literal ids, dynamic prefixes, component-composed suffixes). This
guard executes `scripts/automation/tools/validate.py` in-process to reuse that
resolution verbatim rather than re-deriving an approximation of it; the module
exits on its own verdict, so the `SystemExit` is caught and discarded. The
spec inventory comes from `git ls-files`, never from the disk, so the guard
reads the same set of files whatever tree it runs in.

Exit codes:
    0  measured, every spec sits under a configured testDir and every testid
       it anchors on resolves against the UI source
    1  defect found (spec outside every testDir, or unresolved testid)
    2  nothing measured (no tracked spec, no testDir, or an empty corpus)

Usage:
    python3 scripts/check_playwright_specs.py
"""

import contextlib
import io
import os
import re
import subprocess
import sys
from pathlib import Path, PurePosixPath

REPO_ROOT = Path(__file__).resolve().parents[1]
UI_DIR = "crates/apollia-desktop/ui"
CONFIG = f"{UI_DIR}/playwright.config.ts"
VALIDATOR = "scripts/automation/tools/validate.py"

TESTDIR_RE = re.compile(r'testDir:\s*["\']([^"\']+)["\']')
TESTID_RES = (
    re.compile(r'getByTestId\(\s*["\']([^"\']+)["\']'),
    re.compile(r'data-testid=\\?["\']([A-Za-z0-9_\-./:]+)'),
    re.compile(r'data-testid\^?=\\?["\']([A-Za-z0-9_\-./:]+)'),
)
# Anchors a spec asserts an absence with, by matching nothing on purpose.
NEGATIVE_ANCHORS = {"this-should-match-nothing"}


def tracked(pattern: str) -> list[str]:
    out = subprocess.run(
        ["git", "ls-files", "-z", pattern],
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
        check=True,
    ).stdout
    return [p for p in out.split("\0") if p]


def testid_corpus():
    """Execute the automation validator for its testid resolution.

    Returns its `testid_ok` callable and the size of its literal-id corpus.
    The validator prints its own report and exits; both are discarded here,
    only the corpus it built matters.
    """
    src = (REPO_ROOT / VALIDATOR).read_text(encoding="utf-8")
    ns: dict = {"__name__": "check_playwright_specs.corpus"}
    cwd = os.getcwd()
    os.chdir(REPO_ROOT)
    try:
        with contextlib.redirect_stdout(io.StringIO()):
            with contextlib.suppress(SystemExit):
                exec(compile(src, VALIDATOR, "exec"), ns)
    finally:
        os.chdir(cwd)
    return ns.get("testid_ok"), len(ns.get("static_ids") or ())


def configured_testdirs(config_text: str) -> list[PurePosixPath]:
    dirs = []
    for raw in TESTDIR_RE.findall(config_text):
        dirs.append(PurePosixPath(UI_DIR) / PurePosixPath(raw))
    return dirs


def main() -> int:
    specs = tracked(f"{UI_DIR}/tests/**/*.spec.ts")
    if not specs:
        print(f"NOTHING MEASURED: no tracked spec under {UI_DIR}/tests/")
        return 2

    config_path = REPO_ROOT / CONFIG
    if not config_path.is_file():
        print(f"NOTHING MEASURED: {CONFIG} not found")
        return 2
    testdirs = configured_testdirs(config_path.read_text(encoding="utf-8"))
    if not testdirs:
        print(f"NOTHING MEASURED: no testDir declared in {CONFIG}")
        return 2

    testid_ok, corpus_size = testid_corpus()
    if testid_ok is None or corpus_size == 0:
        print(f"NOTHING MEASURED: {VALIDATOR} produced an empty testid corpus")
        return 2

    problems: list[str] = []
    total_ids = 0
    for spec in specs:
        path = PurePosixPath(spec)
        if not any(td == path.parent or td in path.parents for td in testdirs):
            problems.append(
                f"{spec}: outside every testDir of {CONFIG} "
                f"({', '.join(str(d) for d in testdirs)}); the runner never "
                f"lists it, so it can neither pass nor fail"
            )
        text = (REPO_ROOT / spec).read_text(encoding="utf-8")
        ids = set()
        for rx in TESTID_RES:
            ids.update(rx.findall(text))
        ids -= NEGATIVE_ANCHORS
        total_ids += len(ids)
        for tid in sorted(i for i in ids if not testid_ok(i)):
            problems.append(
                f"{spec}: testid {tid!r} resolves against nothing in "
                f"{UI_DIR}/src; the anchor was removed or renamed"
            )

    if problems:
        for p in problems:
            print(p)
        print(f"{len(problems)} problem(s) across {len(specs)} spec(s).")
        return 1

    print(
        f"{len(specs)} spec(s), {total_ids} testid(s) resolved against a "
        f"corpus of {corpus_size} literal ids, {len(testdirs)} testDir(s)."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
