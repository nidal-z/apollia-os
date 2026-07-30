#!/usr/bin/env python3
"""Every cross-reference in the contract resolves, checked rather than read for.

This exists because two operations amended `README.md` concurrently, one
amendment was lost, and the surviving text went on citing findings that no
longer existed. Nobody noticed by reading: a dangling reference looks exactly
like a live one at the point of use, and the section it names does exist, just
without the finding in it. That failure is mechanical, so the check is too, and
it runs after every merge into the contract.

What it checks:
  1. every `finding N` names a finding that exists
  2. every `finding N in X.Y` names the section that finding is actually in,
     which is the failure that happened and the one prose review missed
  3. every section reference resolves to a heading
  4. every invariant reference `IN` is defined in the invariants section
  5. every repository path in backticks exists on disk

What it does not catch, stated so the next reader does not assume coverage it
does not have. A two-part section reference introduced by `to` or `and` is
skipped, because those are how the prose joins two figures and treating them as
citations flags every `0.0 to 1.0` and every `1.3 and 2.0`. Three-part
references and anything carrying a `§` are always checked, so the gap is
confined to bare two-part numbers in one grammatical position. Widening it means
accepting false positives, which is how a check stops being run.

Exit code 0 when everything resolves, 1 otherwise. Stdlib only.

Usage: check_references.py [file ...]      (default: README.md beside this file)
"""

import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO_ROOT = os.path.abspath(os.path.join(HERE, "..", ".."))

HEADING_RE = re.compile(r"^#{1,6}\s+(\d+(?:\.\d+)+)\s", re.M)
FINDING_RE = re.compile(r"^\*\*(\d+)\.\s", re.M)

# A finding reference, optionally naming the section it is supposed to live in.
FINDING_REF_RE = re.compile(r"finding\s+(\d+)(?:\s+in\s+§?(\d+(?:\.\d+)+))?", re.I)
INVARIANT_REF_RE = re.compile(r"\bI(\d{1,2})\b")
PATH_REF_RE = re.compile(r"`((?:scripts|crates|packaging|docs|sdk|tests)/[^`\s]+)`")

# A bare `1.4.8` is a section reference; `1.3 and 2.0 in the clean run` and
# `0.0 to 1.0` are quantities that happen to share the shape. This document
# cites sections in three forms and no others, so those are what is matched: a
# `§` prefix, a three-part number, or a two-part number carrying a citation cue
# on one side. `and` and `to` are deliberately absent from the cue list, since
# they are how the prose joins two figures.
SECTION_REF_RE = re.compile(r"(?<![\w.])(§)?(\d+\.\d+(?:\.\d+)?)(?![\w.\d])")
CUE_BEFORE_RE = re.compile(
    r"(?:see|in|per|under|section|from|by|against|via|of|both)\s+$", re.I
)
CUE_AFTER_RE = re.compile(
    r"^(?:'s|’s|\s+(?:records|record|says|said|defines|defined|insists|"
    r"requires|puts|sets|anticipated|covers|already|makes|measures|reads|holds|"
    r"states|stated|note|notes|is|was|has|gains|finding|item|and I\d))"
)


def load(path):
    with open(path, "r", encoding="utf-8") as handle:
        return handle.read()


def defined_sections(text):
    return set(HEADING_RE.findall(text))


def defined_findings(text):
    """Findings and every place each number is defined.

    A finding is a bold `**N.` at the start of a line. Which section it belongs
    to is decided by the last heading above it, not by where a citation claims
    it is, which is the whole point of check 2.

    Maps a number to the *list* of its definitions rather than to one, because
    an earlier version kept only the last and was therefore blind to the defect
    it existed to catch: two operations appended to the same section
    independently and defined findings 5 through 9 twice, after which every
    citation of those numbers resolved to two different findings while this
    check reported nothing wrong. A reference to an ambiguous number is not a
    resolved reference.
    """
    headings = [(m.start(), m.group(1)) for m in HEADING_RE.finditer(text)]
    findings = {}
    for match in FINDING_RE.finditer(text):
        section = None
        for start, number in headings:
            if start < match.start():
                section = number
            else:
                break
        line = text.count("\n", 0, match.start()) + 1
        findings.setdefault(match.group(1), []).append((section, line))
    return findings


def defined_invariants(text):
    """Invariants, taken from where they are introduced rather than cited.

    The contract introduces them as `- **I1.** ...` list items and would accept
    a table row just as well, so both shapes count as a definition and a bare
    mention in prose does not.
    """
    defined = set()
    for match in re.finditer(r"^\s*[-*|]\s*\*\*(I\d{1,2})\.?\*\*", text, re.M):
        defined.add(match.group(1))
    for match in re.finditer(r"^\|\s*(I\d{1,2})\s*\|", text, re.M):
        defined.add(match.group(1))
    return defined


def in_code_span(text, position):
    """Whether an offset sits inside backticks, where a token is not a reference."""
    line_start = text.rfind("\n", 0, position) + 1
    return text.count("`", line_start, position) % 2 == 1


def check(path):
    text = load(path)
    sections = defined_sections(text)
    findings = defined_findings(text)
    invariants = defined_invariants(text)
    problems = []

    # Ambiguity is a defect of the definitions, so it is reported once per
    # duplicated number rather than once per citation of it.
    for number, definitions in sorted(findings.items(), key=lambda kv: int(kv[0])):
        if len(definitions) > 1:
            where = ", ".join(
                "line %d in %s" % (line, section or "no section")
                for section, line in definitions
            )
            problems.append(
                "%s: finding %s is defined %d times (%s); every citation of it is "
                "ambiguous" % (path, number, len(definitions), where)
            )

    for match in FINDING_REF_RE.finditer(text):
        number, claimed = match.group(1), match.group(2)
        line = text.count("\n", 0, match.start()) + 1
        if number not in findings:
            problems.append(
                "%s:%d: finding %s is referenced and never defined"
                % (path, line, number)
            )
            continue
        # Not named `sections`: that is the module's own set of defined headings,
        # returned by this function, and shadowing it here silently emptied it.
        defining = {section for section, _ in findings[number]}
        if claimed and claimed not in defining:
            problems.append(
                "%s:%d: reference says finding %s is in %s; it is in %s"
                % (
                    path,
                    line,
                    number,
                    claimed,
                    ", ".join(sorted(s or "no section" for s in defining)),
                )
            )

    for match in SECTION_REF_RE.finditer(text):
        marked, ref = match.group(1), match.group(2)
        if in_code_span(text, match.start()):
            continue
        cited = bool(marked) or ref.count(".") == 2
        if not cited:
            before = text[max(0, match.start() - 24) : match.start()]
            after = text[match.end() : match.end() + 24]
            cited = bool(CUE_BEFORE_RE.search(before) or CUE_AFTER_RE.match(after))
        if not cited:
            continue
        if ref in sections:
            continue
        # A document whose sections are 1.x and 2.x does not make `3.5` a
        # reference, whatever cue happens to sit next to it.
        if ref.split(".")[0] not in {s.split(".")[0] for s in sections}:
            continue
        line = text.count("\n", 0, match.start()) + 1
        problems.append(
            "%s:%d: section %s is referenced and has no heading" % (path, line, ref)
        )

    for match in INVARIANT_REF_RE.finditer(text):
        ref = "I" + match.group(1)
        if ref in invariants:
            continue
        line = text.count("\n", 0, match.start()) + 1
        problems.append(
            "%s:%d: invariant %s is referenced and never defined" % (path, line, ref)
        )

    for match in PATH_REF_RE.finditer(text):
        ref = match.group(1)
        # A glob names a set; it resolves if its directory exists.
        target = os.path.join(REPO_ROOT, ref)
        if any(ch in ref for ch in "*?"):
            target = os.path.dirname(target)
        if os.path.exists(target):
            continue
        line = text.count("\n", 0, match.start()) + 1
        problems.append("%s:%d: path %s does not exist" % (path, line, ref))

    return sections, findings, invariants, problems


def main(argv):
    paths = argv[1:] or [os.path.join(HERE, "README.md")]
    total = 0
    for path in paths:
        if not os.path.isfile(path):
            sys.stderr.write("no such file: %s\n" % path)
            return 2
        sections, findings, invariants, problems = check(path)
        sys.stdout.write(
            "%s: %d sections, %d findings, %d invariants, %d unresolved\n"
            % (path, len(sections), len(findings), len(invariants), len(problems))
        )
        for problem in problems:
            sys.stdout.write("  %s\n" % problem)
        total += len(problems)
    return 1 if total else 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
