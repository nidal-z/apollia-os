#!/usr/bin/env python3
"""Prose rules, runnable outside a commit.

Four of the five rules below already existed. Three lived in the CI job
`Prose Guard`, one in a pre-commit hook. Both fire at a boundary an agent
working inside a lot never crosses: the realisation phase produces prose and
does not commit, so the only thing that could catch a violation ran after the
phase that caused it.

That gap was not hypothetical. A change destined for a public documentation page
carried an em-dash through conception, where the lot file explicitly claimed the
prose added no em-dash, and nothing between the claim and the commit could tell
the difference.

The fifth rule refuses the tracker identifiers and the numbered planning
vocabulary that `docs/agents/FORBIDDEN.md` forbids in any committed file. The
tracker is not published, so every one of those references pointed at something
a reader outside this machine could not open.

Every rule is one entry of `RULES`: its pattern, its label, the paths it
exempts, and whether it also judges file names. A rule that needs an exemption
does not need a second mechanism.

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
from typing import NamedTuple

REPO_ROOT = Path(__file__).resolve().parent.parent

EM_DASH = chr(0x2014)

# The em-dash rule applies to every inventoried file, except the files that
# have to quote the character in order to forbid it.
EM_DASH_EXEMPT = re.compile(
    r"docs/agents/(FORBIDDEN|DOCS-WRITING)\.md$" r"|(^|/)AGENTS\.md$"
)

# One text, valid for `re` and for PCRE2, so the proof published by a lot and
# the rule enforced here cannot drift into two dialects. The scoped `(?i:...)`
# group is what makes that possible: a leading `(?i)` mid-pattern raises at
# import time under Python 3.11 and later.
#
# No bracket-class trick is needed to keep the pattern from matching its own
# source, unlike the three patterns below: each branch is separated from a
# match by the backslash or the alternation bar that follows its prefix. The
# surrounding prose still has to avoid a matchable form, or this file fails the
# rule it carries.
#
# Three of the branches are narrower than the vocabulary they refuse, and each
# bound was measured on this tree rather than guessed:
#
# - `A.<n>` needs a third numeric level, because the CLI end-to-end suite
#   names its sections `A.1` through `A.14` and those labels are the suite's
#   own structure, not a plan. `F`, `E` and `D` carry no such collision and
#   match at two levels.
# - `lot` requires a separator before the number: the concatenated form still
#   names one SDK test identifier, and a guard cannot refuse a form the tree
#   it guards carries.
#   TODO(once `lot3_exports` leaves sdk/tests/test_public_api.py): also refuse
#   the concatenated form.
# - `Story` matches capitalized only: lowercase `story<n>` names 27 Rust test
#   functions across five crates, live code that a prose rule must not judge.
#
# `Phase <n>` and `Batch <n>` are deliberately absent: measured on this tree,
# `Phase` names the Supervisor startup sequence and other in-domain phases on
# some seventy lines, and every `Batch <n>` is fiction served by the seeded
# MCP stub. Both would make the rule fire on compliant content, and a guard
# that fails on compliant input gets switched off.
TRACKING_IDENTIFIER = re.compile(
    r"(?<![A-Za-z0-9])(?:CAP|LOT|GRP)-\d{3}(?![0-9])"
    r"|(?<![A-Za-z0-9])AC-\d+(?![0-9])"
    r"|(?<![A-Za-z0-9])B\.\d+(?![0-9])"
    r"|(?<![A-Za-z0-9])C\.I\.\d+"
    r"|(?<![A-Za-z0-9])[FED]\.\d{1,2}(?:\.\d+)?(?![0-9])"
    r"|(?<![A-Za-z0-9])A\.\d{1,2}\.\d+(?![0-9])"
    r"|(?<![A-Za-z0-9])Story[ _-]\d+"
    r"|(?:follow-up|user) story"
    r"|(?i:(?:sprint|epic)[ _-]?\d+)"
    r"|(?<![A-Za-z0-9])(?i:lot[ _-]\d+)"
)

# The single named hole in that rule: the Figma twin manifest carries the
# identifier inside a schema field and inside dated coverage text, and
# replacing it is a design question rather than a rewrite. Anchored on the full
# relative path on purpose: a directory prefix would also cover the guide and
# the mapping that live beside it, and one of those was a site to correct.
TWIN_MANIFEST_EXEMPT = re.compile(
    r"^crates/apollia-desktop/ui/figma/manifest\.json$"
)


class Rule(NamedTuple):
    pattern: re.Pattern[str]
    label: str
    exempt: re.Pattern[str] | None = None
    names: bool = False


# Bracket classes keep the three middle patterns from matching their own
# literal text here.
RULES = [
    Rule(
        re.compile(re.escape(EM_DASH)),
        "em-dash (U+2014); use a hyphen, comma, or colon",
        EM_DASH_EXEMPT,
    ),
    Rule(
        re.compile(r"/Users/nida[l]"),
        "personal filesystem path",
    ),
    Rule(
        re.compile(r"apollia[.]dev"),
        "dead domain",
    ),
    Rule(
        re.compile(r"github[.]com/(nidal-z|nidalzoumit[a])/"),
        "stale GitHub slug",
    ),
    Rule(
        TRACKING_IDENTIFIER,
        "internal tracker reference; name the condition, not the identifier",
        TWIN_MANIFEST_EXEMPT,
        names=True,
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


def scan(root: Path, files: list[str]) -> list[str]:
    findings: list[str] = []

    # Names first: a file whose name carries the vocabulary is renamed before
    # anyone reads what is inside it.
    for rel in files:
        for rule in RULES:
            if not rule.names:
                continue
            if rule.exempt is not None and rule.exempt.search(rel):
                continue
            if rule.pattern.search(rel):
                findings.append(f"{rel}: {rule.label}, in the file name")

    for rel in files:
        path = root / rel
        try:
            body = path.read_text(encoding="utf-8")
        except (UnicodeDecodeError, OSError):
            continue

        active = [
            rule
            for rule in RULES
            if rule.exempt is None or not rule.exempt.search(rel)
        ]

        for number, line in enumerate(body.splitlines(), start=1):
            for rule in active:
                if rule.pattern.search(line):
                    findings.append(f"{rel}:{number}: {rule.label}")
    return findings


def main() -> int:
    findings = scan(REPO_ROOT, inventoried_files())
    if not findings:
        print("prose: clean")
        return 0
    for finding in findings:
        print(finding)
    print(f"\n{len(findings)} prose violation(s).", file=sys.stderr)
    return 1


if __name__ == "__main__":
    sys.exit(main())
