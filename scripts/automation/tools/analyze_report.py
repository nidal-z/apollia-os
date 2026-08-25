#!/usr/bin/env python3
"""Classify failed steps in a master-det report.json against the script.
Attributes each failure to its domain section (nearest preceding
`screenshot` step whose label starts with 'section-') and prints the step
content + detail, grouped by section, so real bugs vs by-design fast-fails
are quick to triage."""
import argparse
import json
from collections import Counter, defaultdict

_parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
_parser.add_argument(
    "report", nargs="?", default=".apollia-automation/report.json",
    help="report.json written by the automation run",
)
_parser.add_argument(
    "script", nargs="?", default="scripts/automation/master-det.json",
    help="automation script the run executed",
)
_args = _parser.parse_args()
REPORT = _args.report
SCRIPT = _args.script

rep = json.load(open(REPORT))
scr = json.load(open(SCRIPT))
steps = scr["steps"]

# map step index -> section label (nearest preceding section-* screenshot)
section_of = {}
cur = "(preamble)"
for i, s in enumerate(steps):
    if s.get("kind") == "screenshot":
        lbl = s.get("label", "")
        if lbl.startswith("section-"):
            cur = lbl
    section_of[i] = cur

results = rep["steps"]
failed = [r for r in results if not r.get("ok", True)]
print(f"report: {rep['script']}")
print(f"ok={rep['ok']}  steps={len(results)}  FAILED={len(failed)}")
print("=" * 70)

# group failures by section
by_section = defaultdict(list)
for r in failed:
    idx = r["index"]
    by_section[section_of.get(idx, "?")].append(r)

def tgt(s):
    if "testid" in s:
        return f"testid={s['testid']!r}" + (f" nth={s['nth']}" if 'nth' in s else "")
    if "testidPrefix" in s:
        return f"prefix={s['testidPrefix']!r}" + (f" nth={s['nth']}" if 'nth' in s else "")
    if "route" in s:
        return f"route={s['route']!r}"
    if "text" in s:
        return f"text={s['text'][:30]!r}"
    return ""

# heuristic bucket for a failure detail
def bucket(detail):
    d = (detail or "").lower()
    if "timeout" in d or "not found" in d or "no element" in d or "absent" in d:
        return "not-found/timeout"
    if "text" in d and "contain" in d:
        return "text-mismatch"
    return "other"

buckets = Counter()
for sect in sorted(by_section):
    frs = by_section[sect]
    print(f"\n### {sect}  ({len(frs)} failed)")
    for r in frs:
        idx = r["index"]
        s = steps[idx] if idx < len(steps) else {}
        b = bucket(r.get("detail"))
        buckets[b] += 1
        print(f"  [{idx}] {s.get('kind'):10} {tgt(s):40} | {b:18} | {r.get('detail','')[:70]}")

print("\n" + "=" * 70)
print("failure buckets:", dict(buckets))
print("sections with failures:", len(by_section))
