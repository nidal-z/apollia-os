#!/usr/bin/env python3
"""Every Playwright spec is reachable by the runner and anchored in the UI.

Three silent failure modes motivated this guard, all found on the same tree:

  - `playwright.config.ts` declared `testDir: "./tests/perf"`, so 5 of the 7
    specs under `tests/` were invisible to the runner. `npx playwright test
    tests/settings --list` answered "Total: 0 tests in 0 files", which is not
    an error to any caller: 565 lines of test code could neither pass nor fail.
  - 5 of the 48 `data-testid` anchors used by those specs had been removed
    from `src/` by later commits. A renamed or deleted testid does not fail a
    spec at rest; it fails, or silently skips, only at runtime, which nothing
    reached.
  - Two specs drove an application harness that has never existed in any
    commit of `src/`: one seeded `sessionStorage["apollia.perftest.messages"]`
    and waited for a message list to appear, the other navigated to
    `?perf=stream` and waited for a streaming bubble. Both timed out at mount
    on the first run and on every run after it. A testid is not the only
    anchor a spec drops into the application; a storage key and a query
    parameter are anchors too, and neither was being read by anything.

The corpus of resolvable testids is the one the automation validator already
computes (literal ids, dynamic prefixes, component-composed suffixes). This
guard executes `scripts/automation/tools/validate.py` in-process to reuse that
resolution verbatim rather than re-deriving an approximation of it; the module
exits on its own verdict, so the `SystemExit` is caught and discarded. The
spec inventory comes from `git ls-files`, never from the disk, so the guard
reads the same set of files whatever tree it runs in.

Navigation anchors are held two ways, because the tree is not clean on them:

  - a storage key a spec writes must be read somewhere in `src/`. Zero
    tolerance: the corpus holds one such key today and it resolves.
  - a query parameter or URL fragment a spec navigates to must be read
    somewhere in `src/`. `MAX_UNREACHABLE_NAVIGATIONS` is a descending
    ratchet, not a zero: eight sites across four specs address the settings
    pages through a `?route=&sub=` router, and one page through a `#settings`
    fragment, that the application has never implemented. Repairing them is a
    separate piece of work; until then the ceiling forbids a ninth.

What this guard still does not measure is whether a spec has ever *passed*.
Nothing in CI runs the corpus, so a spec can be reachable, correctly anchored,
and red on every run since the day it was written. That is the state of the
five remaining specs, and the ratchet above is the pressure on it rather than
the cure.

Exit codes:
    0  measured, every spec sits under a configured testDir, every testid it
       anchors on resolves, every storage key it writes is read by the UI, and
       the unreachable-navigation count sits at or below the ceiling
    1  defect found
    2  nothing measured (no tracked spec, no testDir, or an empty corpus)

Usage:
    python3 scripts/check_playwright_specs.py
    python3 scripts/check_playwright_specs.py --selftest
"""

import argparse
import contextlib
import io
import os
import re
import subprocess
import sys
from pathlib import Path, PurePosixPath
from urllib.parse import parse_qs, urlparse

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
GOTO_RE = re.compile(r"""\.goto\(\s*["'`]([^"'`]+)["'`]""")
STORAGE_RE = re.compile(
    r"""(?:session|local)Storage\.setItem\(\s*["'`]([^"'`]+)["'`]"""
)
# Anchors a spec asserts an absence with, by matching nothing on purpose.
NEGATIVE_ANCHORS = {"this-should-match-nothing"}

# Navigation sites whose query parameter or fragment nothing in `src/` reads.
# Measured at 8 on 2026-08-27, across the four settings/responsive specs that
# address a `?route=&sub=` router and a `#settings` fragment never built.
# This number descends. Raising it means writing a spec that navigates to a
# place the application cannot be sent to, which is how the two perf specs
# spent their whole life red.
MAX_UNREACHABLE_NAVIGATIONS = 8


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

    Calls its `build_corpus()` and returns a resolver equivalent to the
    validator's own: equality against the literal and composed ids, plus
    the dynamic prefixes (a spec cannot declare `dynamicTestids`, so an
    instance id resolves through the prefix that generates it). Also
    returns the size of the literal-id corpus.
    """
    src = (REPO_ROOT / VALIDATOR).read_text(encoding="utf-8")
    ns: dict = {"__name__": "check_playwright_specs.corpus"}
    cwd = os.getcwd()
    os.chdir(REPO_ROOT)
    try:
        with contextlib.redirect_stdout(io.StringIO()):
            with contextlib.suppress(SystemExit):
                exec(compile(src, VALIDATOR, "exec"), ns)
        build = ns.get("build_corpus")
        if build is None:
            return None, 0
        static_ids, prefixes, composed_ids, _ = build()
    finally:
        os.chdir(cwd)

    def testid_ok(tid: str) -> bool:
        if tid in static_ids or tid in composed_ids:
            return True
        return any(p and tid.startswith(p) for p in prefixes)

    return testid_ok, len(static_ids)


def ui_source_text() -> str:
    """Concatenates every tracked file under the UI `src/` tree.

    A single blob is enough: the questions asked of it are "does anything
    read this query parameter" and "does anything read this storage key",
    which are answered by a literal search and not by a per-file location.
    """
    parts = []
    for rel in tracked(f"{UI_DIR}/src/**"):
        path = REPO_ROOT / rel
        if not path.is_file():
            continue
        with contextlib.suppress(UnicodeDecodeError):
            parts.append(path.read_text(encoding="utf-8"))
    return "\n".join(parts)


def reads_param(src: str, name: str) -> bool:
    """The UI reads query parameter `name` if it passes it to a `.get(...)`."""
    return f'.get("{name}")' in src or f".get('{name}')" in src


def reads_fragment(src: str, frag: str) -> bool:
    """The UI reads fragment `frag` if it compares a hash against it."""
    return f'"#{frag}"' in src or f"'#{frag}'" in src


def reads_storage_key(src: str, key: str) -> bool:
    """The UI reads storage key `key` if the literal appears anywhere."""
    return f'"{key}"' in src or f"'{key}'" in src


def unreachable_navigations(spec_text: str, src: str) -> list[str]:
    """Navigation sites in one spec whose anchors nothing in `src/` reads.

    One entry per `.goto(...)` occurrence, whatever the number of anchors it
    carries: `?route=&sub=` is one place the application cannot be sent to,
    not two.
    """
    found = []
    for url in GOTO_RE.findall(spec_text):
        parsed = urlparse(url)
        dangling = [
            f"query parameter {name!r}"
            for name in sorted(parse_qs(parsed.query))
            if not reads_param(src, name)
        ]
        if parsed.fragment and not reads_fragment(src, parsed.fragment):
            dangling.append(f"fragment '#{parsed.fragment}'")
        if dangling:
            found.append(f"{url} ({', '.join(dangling)})")
    return found


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

    src = ui_source_text()
    if not src:
        print(f"NOTHING MEASURED: no tracked source under {UI_DIR}/src/")
        return 2

    problems: list[str] = []
    total_ids = 0
    total_keys = 0
    unreachable: list[str] = []
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

        keys = sorted(set(STORAGE_RE.findall(text)))
        total_keys += len(keys)
        for key in keys:
            if not reads_storage_key(src, key):
                problems.append(
                    f"{spec}: storage key {key!r} is written by the spec and "
                    f"read by nothing in {UI_DIR}/src; the bridge it seeds "
                    f"does not exist, so the spec waits for a surface that "
                    f"never mounts"
                )

        unreachable.extend(
            f"{spec}: navigates to {site}, which nothing in {UI_DIR}/src reads"
            for site in unreachable_navigations(text, src)
        )

    if len(unreachable) > MAX_UNREACHABLE_NAVIGATIONS:
        problems.extend(unreachable)
        problems.append(
            f"{len(unreachable)} unreachable navigation site(s), ceiling is "
            f"{MAX_UNREACHABLE_NAVIGATIONS}; a spec that navigates somewhere "
            f"the application cannot be sent to fails at mount forever"
        )
    elif len(unreachable) < MAX_UNREACHABLE_NAVIGATIONS:
        problems.append(
            f"{len(unreachable)} unreachable navigation site(s) but "
            f"MAX_UNREACHABLE_NAVIGATIONS is still "
            f"{MAX_UNREACHABLE_NAVIGATIONS}; lower it in this commit so the "
            f"ratchet keeps the ground it just gained"
        )

    if problems:
        for p in problems:
            print(p)
        print(f"{len(problems)} problem(s) across {len(specs)} spec(s).")
        return 1

    print(
        f"{len(specs)} spec(s), {total_ids} testid(s) resolved against a "
        f"corpus of {corpus_size} literal ids, {total_keys} storage key(s) "
        f"read by the UI, {len(unreachable)} unreachable navigation site(s) "
        f"at a ceiling of {MAX_UNREACHABLE_NAVIGATIONS}, "
        f"{len(testdirs)} testDir(s)."
    )
    return 0


def selftest() -> int:
    """Fires every rule on a synthetic input, so a green run is not a mute one.

    A guard whose rules have never been seen to fail proves only that it ran.
    """
    src = 'searchParams.get("tab"); window.location.hash === "#design"; "kept"'
    failures = []

    if not reads_param(src, "tab"):
        failures.append("reads_param missed a parameter the UI reads")
    if reads_param(src, "route"):
        failures.append("reads_param accepted a parameter nothing reads")
    if not reads_fragment(src, "design"):
        failures.append("reads_fragment missed a fragment the UI reads")
    if reads_fragment(src, "settings"):
        failures.append("reads_fragment accepted a fragment nothing reads")
    if not reads_storage_key(src, "kept"):
        failures.append("reads_storage_key missed a key the UI reads")
    if reads_storage_key(src, "apollia.perftest.messages"):
        failures.append("reads_storage_key accepted a key nothing reads")

    spec = '.goto("/?route=settings&sub=stt");\n.goto("/#design");\n.goto("/")'
    found = unreachable_navigations(spec, src)
    if len(found) != 1:
        failures.append(f"unreachable_navigations found {len(found)}, want 1")

    dead = 'sessionStorage.setItem("apollia.perftest.messages", "[]")'
    if STORAGE_RE.findall(dead) != ["apollia.perftest.messages"]:
        failures.append("STORAGE_RE missed a sessionStorage bridge")

    if failures:
        for f in failures:
            print(f"SELFTEST FAILED: {f}")
        return 1
    print("selftest: 8 rule(s) fired on synthetic input, none misfired.")
    return 0


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument(
        "--selftest",
        action="store_true",
        help="run every rule against synthetic input and exit",
    )
    args = parser.parse_args()
    sys.exit(selftest() if args.selftest else main())
