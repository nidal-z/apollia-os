#!/usr/bin/env python3
"""Run the coverage instrument for the API operations no CLI leaf reaches.

`cargo test <filter>` exits 0 when the filter matches nothing, so a renamed
module or a mistyped filter turns "nothing was measured" into a green run. This
wrapper reads the count cargo reports and refuses that reading.

Exit codes:
  0  every selected test ran and passed
  1  at least one selected test failed
  2  nothing was measured (build failure, no result line, or zero tests run)
"""

# REASON: print-call: this module is a command-line entry point; print() is
# its output.

import argparse
import re
import subprocess
import sys
from pathlib import Path

CRATE = "apollia-runtime"

# The two filters that select the instrument: the module that probes the eleven
# operations, and the two session-todo tests that live with the chat harness.
DEFAULT_FILTERS = [
    "api::unreached_by_cli",
    "api::routes_chat::tests::test_get_session_todo",
]

# `test result: ok. 16 passed; 0 failed; 0 ignored; ...`
RESULT_RE = re.compile(
    r"test result: (?P<verdict>\w+)\. (?P<passed>\d+) passed; (?P<failed>\d+) failed"
)


def repo_root() -> Path:
    """Repository root, four levels above this file."""
    return Path(__file__).resolve().parents[4]


def run_cargo(root: Path, filters: list[str]) -> tuple[int, str]:
    """Run the filtered test binary and return (exit code, combined output)."""
    cmd = ["cargo", "test", "-p", CRATE, "--lib", "--", *filters]
    proc = subprocess.run(
        cmd, cwd=str(root), capture_output=True, text=True, check=False
    )
    return proc.returncode, proc.stdout + proc.stderr


def measure(output: str) -> tuple[int, int] | None:
    """Total (passed, failed) across every result line, or None if there is none."""
    matches = list(RESULT_RE.finditer(output))
    if not matches:
        return None
    passed = sum(int(m.group("passed")) for m in matches)
    failed = sum(int(m.group("failed")) for m in matches)
    return passed, failed


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "filters",
        nargs="*",
        default=DEFAULT_FILTERS,
        help="test name filters (default: the instrument's two filters)",
    )
    parser.add_argument(
        "--verbose", action="store_true", help="echo the cargo output"
    )
    args = parser.parse_args()
    filters = args.filters or DEFAULT_FILTERS

    code, output = run_cargo(repo_root(), filters)
    if args.verbose:
        print(output)

    counted = measure(output)
    if counted is None:
        print(f"nothing measured: cargo produced no test result line (exit {code})")
        print(output[-2000:])
        return 2

    passed, failed = counted
    if passed + failed == 0:
        print(f"nothing measured: filters {filters} selected no test")
        return 2

    print(f"tests measured: {passed + failed} ({passed} passed, {failed} failed)")
    for name in filters:
        print(f"  filter: {name}")
    if failed or code != 0:
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
