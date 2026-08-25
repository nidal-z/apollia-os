#!/usr/bin/env python3
"""Pin the coverage of the data-testid corpus by the automation scripts.

`scripts/automation/tools/validate.py` answers "does every anchor a script
names still resolve?". This guard pins the two properties the validator's
answer rests on, both measured from the same corpus builder:

  1. No masking. An anchor a script references as an exact id must resolve as
     an exact id and nothing else. An exact id that would also resolve through
     a composed suffix is an anchor whose literal can vanish from the source
     with every guard staying green: 57 anchors sat in that state while the
     resolution rule accepted any dynamic prefix as a fallback.

  2. A declared ratchet on unreached anchors. 679 of the 1247 literal
     `data-testid` anchors of the UI are referenced by no script; they can
     disappear without any guard noticing, which is a measure of blindness,
     not a defect. The count may only go down: a new surface shipped without
     a script raises it and turns this guard red. When scripts extend the
     corpus, lower NEVER_REACHED_MAX to the new measure.

Usage:
    python3 scripts/check_testid_anchors.py [--list-masked] [--list-never]

Exit codes: 0 both properties hold, 1 an unresolved or masked anchor exists or
the ratchet is exceeded, 2 nothing measured (empty corpus or no script).
"""

import glob
import importlib.util
import json
import os
import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]

# Ratchet: literal anchors referenced by no script, measured 2026-08-25.
# Descending only. Lower it when scripts reach more of the interface.
NEVER_REACHED_MAX = 679

LITERAL = re.compile(r'data-testid="([A-Za-z0-9_\-./:]+)"')


def load_validator():
    path = REPO_ROOT / "scripts/automation/tools/validate.py"
    spec = importlib.util.spec_from_file_location("automation_validate", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def main() -> int:
    os.chdir(REPO_ROOT)
    val = load_validator()
    static_ids, prefixes, composed_ids, _ = val.build_corpus()

    used: set[str] = set()
    used_prefix: set[str] = set()
    declared_dynamic: set[str] = set()
    script_files = sorted(glob.glob("scripts/automation/*.json"))
    for f in script_files:
        try:
            data = json.load(open(f))
        except (OSError, ValueError):
            continue  # validate.py owns the parse verdict
        declared_dynamic.update(data.get("dynamicTestids", []))
        for step in data.get("steps", []):
            if step.get("testid"):
                used.add(step["testid"])
            if step.get("testidPrefix"):
                used_prefix.add(step["testidPrefix"])

    if not static_ids or not script_files:
        print("NOTHING MEASURED: empty corpus or no script "
              "(run from the repository root)")
        return 2

    exact = {t for t in used if t in static_ids}
    composed = {t for t in used - exact if t in composed_ids}
    dynamic = {t for t in used - exact - composed if t in declared_dynamic}
    unresolved = sorted(used - exact - composed - dynamic)

    # An exact anchor that another resolution path would still accept after
    # its literal vanished. The composed set is the only other equality path;
    # a declared-dynamic exact id is the same defect through the declaration.
    masked = sorted(t for t in exact if t in composed_ids or t in declared_dynamic)

    literal_src: dict[str, str] = {}
    for path in glob.glob("crates/apollia-desktop/ui/src/**/*.svelte", recursive=True):
        try:
            txt = open(path, encoding="utf-8").read()
        except OSError:
            continue
        for m in LITERAL.finditer(txt):
            literal_src.setdefault(m.group(1), path)
    never = sorted(
        t for t in literal_src
        if t not in used and not any(t.startswith(p) for p in used_prefix)
    )

    print(f"script testids: {len(used)} distinct; exact={len(exact)} "
          f"composed={len(composed)} declared-dynamic={len(dynamic)} "
          f"unresolved={len(unresolved)}; testidPrefix steps: {len(used_prefix)} distinct")
    print(f"masked exact testids: {len(masked)}")
    if "--list-masked" in sys.argv:
        for t in masked:
            print("   ", t)
    print(f"literal data-testid in src/**/*.svelte: {len(literal_src)}; "
          f"referenced by no script: {len(never)} (ratchet {NEVER_REACHED_MAX})")
    if "--list-never" in sys.argv:
        for t in never:
            print("   ", t, literal_src[t])

    failed = False
    for t in unresolved:
        print(f"UNRESOLVED: {t}")
        failed = True
    for t in masked:
        print(f"MASKED: {t} (exact literal that another path would still resolve)")
        failed = True
    if len(never) > NEVER_REACHED_MAX:
        print(f"RATCHET EXCEEDED: {len(never)} unreached anchors > {NEVER_REACHED_MAX}. "
              f"A new surface shipped without a script, or an existing script lost "
              f"a reference. Write the script that reaches it; the ratchet only goes down")
        failed = True
    if failed:
        return 1
    print("OK: 0 unresolved, 0 masked, ratchet respected")
    return 0


if __name__ == "__main__":
    sys.exit(main())
