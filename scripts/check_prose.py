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

Scans tracked files only, which is what a clean CI checkout sees. Ignored trees
such as internal notes hold prose that follows its own conventions and would
otherwise report as failures that no commit could ever produce.

Exit code 0 when clean, 1 when a rule fires.
"""

import re
import subprocess
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent

EM_DASH = chr(0x2014)

# The em-dash rule applies to prose the reader meets, not to the files that have
# to quote the character in order to forbid it.
EM_DASH_SUFFIXES = (".md", ".rs", ".py", ".svelte")
EM_DASH_EXEMPT = re.compile(r"docs/agents/(FORBIDDEN|DOCS-WRITING)\.md$|(^|/)AGENTS\.md$")

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


def tracked_files() -> list[str]:
    out = subprocess.run(
        ["git", "ls-files", "-z"],
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
        check=True,
    ).stdout
    return [p for p in out.split("\0") if p]


def scan() -> list[str]:
    findings: list[str] = []
    for rel in tracked_files():
        path = REPO_ROOT / rel
        try:
            body = path.read_text(encoding="utf-8")
        except (UnicodeDecodeError, OSError):
            continue

        check_em_dash = rel.endswith(EM_DASH_SUFFIXES) and not EM_DASH_EXEMPT.search(rel)

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
