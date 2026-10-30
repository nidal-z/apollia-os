#!/usr/bin/env python3
"""npm audit of the desktop UI fails on every advisory it does not accept in writing.

Reads the JSON report of `npm audit --omit=dev --json` and the accepted list
`crates/apollia-desktop/ui/audit-ignore.json`, the same model as the ignore
list of `deny.toml`: an advisory stays accepted only with the reason it cannot
reach Apollia and the condition that lifts it. Any advisory not listed fails
the check, whatever its severity; an accepted entry the report no longer
contains is named, so the list only shrinks.

Usage: python3 scripts/check_npm_audit.py <report.json> [<audit-ignore.json>]
"""

import argparse
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DEFAULT_IGNORE = ROOT / "crates" / "apollia-desktop" / "ui" / "audit-ignore.json"
ADVISORY_ID = re.compile(r"GHSA(?:-[0-9a-z]{4}){3}")


def advisories(report: dict) -> dict[str, set[str]]:
    """Advisory identifier to the packages it is reported on."""
    found: dict[str, set[str]] = {}
    for name, entry in report.get("vulnerabilities", {}).items():
        for via in entry.get("via", []):
            if not isinstance(via, dict):
                continue
            match = ADVISORY_ID.search(str(via.get("url", "")))
            key = match.group(0) if match else str(via.get("source", via.get("title", name)))
            found.setdefault(key, set()).add(name)
    return found


def accepted(ignore_path: Path) -> dict[str, dict]:
    """Accepted advisories by identifier; an entry without a reason or a lift condition is refused."""
    entries = json.loads(ignore_path.read_text(encoding="utf-8")).get("advisories", [])
    out: dict[str, dict] = {}
    for entry in entries:
        missing = [field for field in ("id", "package", "reason", "lift") if not entry.get(field)]
        if missing:
            raise ValueError(f"{ignore_path}: an entry lacks {', '.join(missing)}: {entry}")
        out[entry["id"]] = entry
    return out


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("report", type=Path, help="output of npm audit --omit=dev --json")
    parser.add_argument("ignore", nargs="?", type=Path, default=DEFAULT_IGNORE)
    args = parser.parse_args()

    report = json.loads(args.report.read_text(encoding="utf-8"))
    if "error" in report:
        print(f"FAIL: npm audit did not produce a report: {report['error']}")
        return 1
    found = advisories(report)
    allowed = accepted(args.ignore)

    unaccepted = sorted(set(found) - set(allowed))
    stale = sorted(set(allowed) - set(found))
    for key in sorted(set(found) & set(allowed)):
        print(f"accepted: {key} on {', '.join(sorted(found[key]))} (lift: {allowed[key]['lift']})")
    for key in stale:
        print(f"no longer encountered, remove it from {args.ignore.name}: {key}")
    for key in unaccepted:
        print(f"FAIL: {key} on {', '.join(sorted(found[key]))} is not accepted in {args.ignore.name}")
    if unaccepted:
        return 1
    print(f"OK: {len(found)} advisory(ies), all accepted in writing")
    return 0


if __name__ == "__main__":
    sys.exit(main())
