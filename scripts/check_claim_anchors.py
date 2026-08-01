#!/usr/bin/env python3
"""Re-anchoring pass: check the markers against the corpus as it is now.

`check_claims.py` replays the evidence. That is not the same question as
whether the markers still sit where they are supposed to, and waves 7 to 9
moved, translated, merged and deleted exactly the files that carry them. A
marker can survive its own page being split in two, land in the half that no
longer states the claim, and the replay stays green: it only asks whether *some*
file carries the id.

So this looks from both ends, and at placement rather than existence:

1. Every claim id has at least one marker. (The replay covers this; repeated
   here so one run answers the whole question.)
2. Every marker corresponds to a claim. An orphan marker is a paragraph that
   believes it is guarded and is not.
3. A claim anchored in the English corpus is also anchored in the French mirror,
   or is deliberately English-only. The translation carried the markers by
   instruction; this is where that instruction is verified rather than trusted.
4. No marker sits in the rulebook, which leaves the repository when the public
   one is seeded and would take the anchor with it.
5. A marker is followed by prose within a few lines. A marker left at the end of
   a file, or immediately before another marker, guards nothing.
"""

import re
import sys
import tomllib
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
EN = REPO / "docs/site/docs"
FR = REPO / "docs/site/i18n/fr/docusaurus-plugin-content-docs/current"
ADR = REPO / "docs/adr"
RULEBOOK = [REPO / "docs/agents"] + [REPO / p for p in ("AGENTS.md",)]

MARKER = re.compile(r"<!--\s*claim:([a-z0-9-]+)\s*-->")


def markers_in(root: Path) -> dict[str, list[tuple[Path, int]]]:
    found: dict[str, list[tuple[Path, int]]] = {}
    paths = [root] if root.is_file() else sorted(root.rglob("*.md"))
    for path in paths:
        if not path.is_file():
            continue
        for n, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
            for cid in MARKER.findall(line):
                found.setdefault(cid, []).append((path, n))
    return found


def merge(*dicts):
    out: dict[str, list] = {}
    for d in dicts:
        for k, v in d.items():
            out.setdefault(k, []).extend(v)
    return out


def main() -> int:
    claims = tomllib.loads((REPO / "docs/CLAIMS.toml").read_text())["claim"]
    ids = {c["id"] for c in claims}

    en, fr, adr = markers_in(EN), markers_in(FR), markers_in(ADR)
    rule = merge(*(markers_in(p) for p in RULEBOOK))
    everywhere = merge(en, fr, adr, rule)

    problems: list[str] = []

    for cid in sorted(ids - everywhere.keys()):
        problems.append(f"{cid}: no marker anywhere. The claim guards nothing.")

    for cid in sorted(everywhere.keys() - ids):
        where = ", ".join(f"{p.relative_to(REPO)}:{n}" for p, n in everywhere[cid])
        problems.append(
            f"{cid}: marker present with no entry in CLAIMS.toml ({where}). "
            f"The paragraph reads as guarded and is not."
        )

    for cid in sorted(rule):
        where = ", ".join(f"{p.relative_to(REPO)}:{n}" for p, n in rule[cid])
        problems.append(
            f"{cid}: marker sits in the rulebook ({where}), which leaves the "
            f"repository when the public one is seeded, taking the anchor with it."
        )

    # Mirror coverage: a claim anchored on a translated page should be anchored
    # on its translation too.
    #
    # Count what this rule actually looked at, and say so. The mirror carries
    # operator-help alone, while every marker today sits in explanation,
    # architecture, how-to or reference, none of which is translated. So the
    # rule has nothing to check, and a bare "green" would report that as
    # verified. A check with no coverage must announce its own emptiness,
    # otherwise it is the thing this whole pass exists to remove.
    mirror_checked = 0
    for cid in sorted(set(en) & ids):
        if cid in fr:
            continue
        pages = [p for p, _ in en[cid]]
        mirrored = [p for p in pages if (FR / p.relative_to(EN)).is_file()]
        mirror_checked += len(mirrored)
        if mirrored:
            names = ", ".join(str(p.relative_to(EN)) for p in mirrored)
            problems.append(
                f"{cid}: anchored in {names}, which has a French mirror, but the "
                f"mirror carries no marker. The translation dropped it."
            )

    # Placement: a marker must be followed by prose it can plausibly guard.
    for cid, sites in everywhere.items():
        for path, n in sites:
            lines = path.read_text(encoding="utf-8").splitlines()
            after = [l.strip() for l in lines[n : n + 4]]
            if not any(l and not MARKER.search(l) and not l.startswith("<!--") for l in after):
                problems.append(
                    f"{cid}: marker at {path.relative_to(REPO)}:{n} is followed by "
                    f"nothing it could guard."
                )

    print(f"{len(ids)} claims, {sum(len(v) for v in everywhere.values())} markers")

    # Coverage per zone, counted rather than asserted. A guard that reports
    # "pass" without saying what it looked at is the failure this whole pass
    # exists to remove, and a hardcoded list of zones goes stale the first time
    # someone adds a marker somewhere new.
    zones: dict[str, list[int]] = {}
    for root, prefix in ((EN, ""), (FR, "i18n/fr "), (ADR, None)):
        for path in sorted(root.rglob("*.md")) if root.is_dir() else []:
            rel = path.relative_to(root)
            if prefix is None:
                zone = "docs/adr"
            else:
                zone = prefix + (rel.parts[0] if len(rel.parts) > 1 else "(root pages)")
            counts = zones.setdefault(zone, [0, 0])
            counts[0] += 1
            counts[1] += len(MARKER.findall(path.read_text(encoding="utf-8")))

    print("\n  zone                       pages  markers")
    for zone in sorted(zones):
        pages, marks = zones[zone]
        flag = "   <- no coverage" if marks == 0 else ""
        print(f"  {zone:<26} {pages:>5}  {marks:>7}{flag}")
    print("\n  A zone at zero is not necessarily wrong: it may hold no capability")
    print("  claim under the criterion. It is wrong when nobody has looked, which")
    print("  is why the number is printed rather than the verdict.")

    if mirror_checked:
        print(f"\n  mirror rule: {mirror_checked} anchored page(s) compared with their translation")
    else:
        print("\n  mirror rule: NO COVERAGE. No marked page has a translation yet.")

    if problems:
        print(f"\n{len(problems)} problem(s):\n", file=sys.stderr)
        for p in problems:
            print(f"  {p}\n", file=sys.stderr)
        return 1
    print("\nevery marker is placed, matched, and mirrored where it should be")
    return 0


if __name__ == "__main__":
    sys.exit(main())
