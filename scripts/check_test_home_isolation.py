#!/usr/bin/env python3
"""Prove that a test run leaves the profile directory of its HOME untouched.

Twenty-eight unit tests of `apollia-runtime` opened `governance.db` inside the
real `~/.apollia` of whoever ran `cargo test`: the chat manager resolved the
path from the process home directory at call time, and the test mount
substituted neither the path nor `HOME`. The write was invisible to every
existing gate because `cargo test` is green whether the database exists or not,
and a `find -newermt` sweep cannot see a WAL file that is created and deleted
inside one test.

This guard measures the property instead of trusting the code. It builds a
sentinel HOME in a temporary directory, seeds `.apollia/governance.db` in it,
snapshots the whole `.apollia` inventory (names, sizes, mtimes, directory
mtimes), runs the tests with `HOME` pointing at the sentinel, and compares the
inventory afterwards. A transient file leaves its parent directory's mtime
behind, so even a created-then-deleted WAL is seen.

Two modes:

  - default: scoped to the historical offender. Builds the `apollia-runtime`
    unit-test binary (`cargo test -p apollia-runtime --no-run`) and runs it
    under the sentinel.
  - `--wrap CMD...`: envelope for a whole command, e.g.
    `--wrap cargo test --workspace --no-fail-fast`. The command runs with
    `HOME` on the sentinel (CARGO_HOME and RUSTUP_HOME are pinned to their
    real resolution first, so cargo does not re-bootstrap into the sentinel),
    its output passes through, and the verdict line
    `HOME-SENTINEL: CLEAN` or `HOME-SENTINEL: CHANGED <n>` is appended for
    `scripts/worktree_verdicts.py` to measure. A non-zero command exit wins
    over the sentinel verdict; a clean command exit with a dirty sentinel
    exits 1.

Exit codes:
    0   the sentinel is untouched (and, under --wrap, the command succeeded)
    1   the sentinel was written to, or a --selftest case failed
    2   nothing was measured: cargo or the test binary is unavailable, or the
        binary died without printing a test summary

In the default (scoped) mode a red test suite does not decide the verdict:
the suite has its own gate, and letting it decide here would make the
sentinel hostage to any flaky test. The exit is reported as a note.

`--selftest` replays a positive and a negative control on a temporary
fixture, never on the tree: a child process that writes under
`$HOME/.apollia` must turn the verdict red, one that only touches its own
temporary directory must leave it green, and a child exit code must pass
through unchanged.
"""

import argparse
import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]

SENTINEL_DB = "governance.db"
MARKER = "sentinel.marker"

EXECUTABLE_LINE = re.compile(r"^\s*Executable unittests src/lib\.rs \((.+)\)\s*$", re.M)


def build_sentinel(base: Path) -> Path:
    """Create `<base>/home/.apollia` with a seeded governance database."""
    home = base / "home"
    apollia = home / ".apollia"
    apollia.mkdir(parents=True)
    (apollia / SENTINEL_DB).touch()
    (apollia / MARKER).write_text("home isolation sentinel\n", encoding="utf-8")
    return home


def inventory(apollia: Path) -> dict[str, tuple[str, int, int]]:
    """Every entry under `.apollia`: kind, size (files only), mtime in ns.

    Directory mtimes are part of the record on purpose: a file created and
    deleted between the two snapshots is gone from the listing but has moved
    its parent directory's mtime, which is exactly how the transient WAL
    files of the original defect were caught.
    """
    entries: dict[str, tuple[str, int, int]] = {}
    stat = apollia.stat()
    entries["."] = ("dir", 0, stat.st_mtime_ns)
    for path in sorted(apollia.rglob("*")):
        rel = str(path.relative_to(apollia))
        st = path.lstat()
        if path.is_dir() and not path.is_symlink():
            entries[rel] = ("dir", 0, st.st_mtime_ns)
        else:
            entries[rel] = ("file", st.st_size, st.st_mtime_ns)
    return entries


def diff(before: dict, after: dict) -> list[str]:
    lines: list[str] = []
    for name in sorted(set(before) | set(after)):
        if name not in before:
            kind, size, _ = after[name]
            lines.append(f"created  {name} ({kind}, {size} bytes)")
        elif name not in after:
            lines.append(f"deleted  {name}")
        elif before[name] != after[name]:
            b, a = before[name], after[name]
            what = []
            if b[1] != a[1]:
                what.append(f"size {b[1]} -> {a[1]}")
            if b[2] != a[2]:
                what.append("mtime moved")
            lines.append(f"changed  {name} ({', '.join(what)})")
    return lines


def run_under_sentinel(command: list[str], passthrough: bool) -> tuple[int, int, list[str]]:
    """Run `command` with HOME on a fresh sentinel; report (exit, changes, detail)."""
    with tempfile.TemporaryDirectory(prefix="apollia-home-sentinel-") as tmp:
        home = build_sentinel(Path(tmp))
        apollia = home / ".apollia"
        before = inventory(apollia)

        env = dict(os.environ)
        # Resolved against the real home, before HOME is overridden, so a
        # wrapped cargo neither re-downloads a registry nor writes one into
        # the sentinel (which would be a false red about the tests).
        env.setdefault("CARGO_HOME", str(Path.home() / ".cargo"))
        env.setdefault("RUSTUP_HOME", str(Path.home() / ".rustup"))
        env["HOME"] = str(home)

        if passthrough:
            proc = subprocess.run(command, env=env, check=False)
            out_code = proc.returncode
        else:
            proc = subprocess.run(command, env=env, check=False, capture_output=True, text=True)
            out_code = proc.returncode
            sys.stdout.write(proc.stdout)
            sys.stderr.write(proc.stderr)

        after = inventory(apollia)
        detail = diff(before, after)
        return out_code, len(detail), detail


def report_sentinel(changes: int, detail: list[str]) -> None:
    if changes == 0:
        print("HOME-SENTINEL: CLEAN")
        return
    print(f"HOME-SENTINEL: CHANGED {changes}")
    for line in detail:
        print(f"  {line}")


def runtime_unittest_binary() -> Path | None:
    """Build the apollia-runtime unit tests and return the binary path."""
    build = subprocess.run(
        ["cargo", "test", "-p", "apollia-runtime", "--no-run"],
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
        check=False,
    )
    if build.returncode != 0:
        sys.stderr.write(build.stderr)
        return None
    match = EXECUTABLE_LINE.search(build.stderr)
    if match is None:
        return None
    path = Path(match.group(1))
    if not path.is_absolute():
        path = REPO_ROOT / path
    return path if path.is_file() else None


def scoped_check() -> int:
    binary = runtime_unittest_binary()
    if binary is None:
        print(
            "nothing measured: the apollia-runtime unit-test binary could not "
            "be built or located",
            file=sys.stderr,
        )
        return 2
    code, changes, detail, reported = run_captured([str(binary)])
    if not reported:
        print(
            f"nothing measured: the test binary exited {code} without printing "
            "a test summary, so the sentinel saw no test run",
            file=sys.stderr,
        )
        return 2
    report_sentinel(changes, detail)
    if code != 0:
        # Reported failures belong to the test suite's own gate. This guard
        # judges home isolation alone, so a red test does not decide here;
        # deciding on it would make the sentinel hostage to any flaky test.
        print(f"note: test binary exited {code} (judged by the test gate, not here)")
    return 1 if changes else 0


def run_captured(command: list[str]) -> tuple[int, int, list[str], bool]:
    """`run_under_sentinel` for the scoped mode, plus a did-it-report flag."""
    with tempfile.TemporaryDirectory(prefix="apollia-home-sentinel-") as tmp:
        home = build_sentinel(Path(tmp))
        apollia = home / ".apollia"
        before = inventory(apollia)
        env = dict(os.environ)
        env["HOME"] = str(home)
        proc = subprocess.run(command, env=env, check=False, capture_output=True, text=True)
        sys.stdout.write(proc.stdout)
        sys.stderr.write(proc.stderr)
        after = inventory(apollia)
        detail = diff(before, after)
        reported = "test result:" in proc.stdout
        return proc.returncode, len(detail), detail, reported


def wrap(command: list[str]) -> int:
    code, changes, detail = run_under_sentinel(command, passthrough=True)
    report_sentinel(changes, detail)
    if code != 0:
        return code
    return 1 if changes else 0


def selftest() -> int:
    failures = 0

    def case(name: str, ok: bool, detail: str) -> None:
        nonlocal failures
        if ok:
            print(f"  ok    {name}")
        else:
            print(f"  FAIL  {name}: {detail}")
            failures += 1

    python = sys.executable

    clean = [python, "-c", "pass"]
    code, changes, _ = run_under_sentinel(clean, passthrough=False)
    case(
        "a child that leaves HOME alone is green",
        code == 0 and changes == 0,
        f"exit {code}, {changes} change(s)",
    )

    writer = [
        python,
        "-c",
        "import os, pathlib; pathlib.Path(os.environ['HOME'], "
        "'.apollia', 'selftest-evil').write_text('x')",
    ]
    code, changes, detail = run_under_sentinel(writer, passthrough=False)
    case(
        "a child that writes under $HOME/.apollia is seen",
        code == 0 and changes >= 1 and any("selftest-evil" in line for line in detail),
        f"exit {code}, {changes} change(s), {detail!r}",
    )

    toucher = [
        python,
        "-c",
        "import os, pathlib; pathlib.Path(os.environ['HOME'], "
        f"'.apollia', '{SENTINEL_DB}').touch()",
    ]
    code, changes, detail = run_under_sentinel(toucher, passthrough=False)
    case(
        "a bare mtime move on the seeded database is seen",
        code == 0 and changes >= 1,
        f"exit {code}, {changes} change(s), {detail!r}",
    )

    transient = [
        python,
        "-c",
        "import os, pathlib; p = pathlib.Path(os.environ['HOME'], "
        "'.apollia', 'governance.db-wal'); p.write_text('x'); p.unlink()",
    ]
    code, changes, detail = run_under_sentinel(transient, passthrough=False)
    case(
        "a file created then deleted leaves a trace (parent mtime)",
        code == 0 and changes >= 1,
        f"exit {code}, {changes} change(s), {detail!r}",
    )

    failing = [python, "-c", "raise SystemExit(3)"]
    code, changes, _ = run_under_sentinel(failing, passthrough=False)
    case(
        "a failing child's exit code passes through",
        code == 3 and changes == 0,
        f"exit {code}, {changes} change(s)",
    )

    if failures:
        print(f"{failures} selftest case(s) failed", file=sys.stderr)
        return 1
    print("selftest: 5 cases green")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    group = parser.add_mutually_exclusive_group()
    group.add_argument(
        "--wrap",
        nargs=argparse.REMAINDER,
        metavar="CMD",
        help="run CMD under a sentinel HOME and append the verdict line",
    )
    group.add_argument(
        "--selftest",
        action="store_true",
        help="replay a positive and a negative control on a temporary fixture",
    )
    args = parser.parse_args()

    if args.selftest:
        return selftest()
    if args.wrap is not None:
        if not args.wrap:
            parser.error("--wrap needs a command to run")
        return wrap(args.wrap)
    return scoped_check()


if __name__ == "__main__":
    sys.exit(main())
