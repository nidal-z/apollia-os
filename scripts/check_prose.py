#!/usr/bin/env python3
"""Prose rules, runnable outside a commit.

The four rules below already existed. Three lived in the CI job `Prose Guard`,
one in a pre-commit hook. Both fire at a boundary an agent working inside a lot
never crosses: the realisation phase produces prose and does not commit, so the
only thing that could catch a violation ran after the phase that caused it.

That gap was not hypothetical. A change destined for a public documentation page
carried an em-dash through conception, where the lot file explicitly claimed the
prose added no em-dash, and nothing between the claim and the commit could tell
the difference.

Scans the union of tracked files and untracked files git does not ignore, so
the file a lot creates before its first commit is judged in the phase that
produces it. Ignored trees such as internal notes stay out: they hold prose
that follows its own conventions and would otherwise report as failures that
no commit could ever produce.

Exit code 0 when clean, 1 when a rule fires.
"""

import re
import subprocess
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent

EM_DASH = chr(0x2014)

# The em-dash rule applies to every inventoried file, except the files that
# have to quote the character in order to forbid it.
EM_DASH_EXEMPT = re.compile(
    r"docs/agents/(FORBIDDEN|DOCS-WRITING)\.md$"
    r"|(^|/)AGENTS\.md$"
    r"|^\.github/workflows/ci\.yml$"
)

# Bracket classes keep each pattern from matching its own literal text here.
PATTERNS = [
    (
        re.compile(r"/Users/nida[l]"),
        "personal filesystem path",
    ),
    (
        re.compile(r"apollia[.]dev"),
        "dead domain",
    ),
    (
        re.compile(r"github[.]com/(nidal-z|nidalzoumit[a])/"),
        "stale GitHub slug",
    ),
]


def inventoried_files() -> list[str]:
    def ls_files(*args: str) -> list[str]:
        out = subprocess.run(
            ["git", "ls-files", "-z", *args],
            cwd=REPO_ROOT,
            capture_output=True,
            text=True,
            check=True,
        ).stdout
        return [p for p in out.split("\0") if p]

    tracked = ls_files()
    seen = set(tracked)
    untracked = [
        p for p in ls_files("--others", "--exclude-standard") if p not in seen
    ]
    return tracked + untracked


def scan() -> list[str]:
    findings: list[str] = []
    for rel in inventoried_files():
        path = REPO_ROOT / rel
        try:
            body = path.read_text(encoding="utf-8")
        except (UnicodeDecodeError, OSError):
            continue

        check_em_dash = not EM_DASH_EXEMPT.search(rel)

        for number, line in enumerate(body.splitlines(), start=1):
            if check_em_dash and EM_DASH in line:
                findings.append(
                    f"{rel}:{number}: em-dash (U+2014); use a hyphen, comma, or colon"
                )
            for pattern, label in PATTERNS:
                if pattern.search(line):
                    findings.append(f"{rel}:{number}: {label}")
    return findings


def main() -> int:
    findings = scan()
    if not findings:
        print("prose: clean")
        return 0
    for finding in findings:
        print(finding)
    print(f"\n{len(findings)} prose violation(s).", file=sys.stderr)
    return 1


if __name__ == "__main__":
    sys.exit(main())
