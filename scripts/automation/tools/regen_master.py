#!/usr/bin/env python3
"""Regenerate master-det.json from the per-page <page>-det.json scripts.

master-det = head (section-00 start + global onboarding dismiss) then, for each
page in section order: [screenshot section-NN-<page>, goto dashboard, waitFor
app-main] + the page's steps minus its leading 2-step onboarding preamble
(onboarding-skip + waitGone). The master's `dynamicTestids` declaration is the
union of the pages' declarations, since its steps are the union of theirs.

Validates against the current committed master-det unless --write is passed.
"""
import json, sys

SCRIPTS = "scripts/automation"
# section order (from the section-NN markers), name maps to <name>-det.json
ORDER = [
    "nav-det", "dashboard-det", "chat-det", "agents-det", "projects-det",
    "tasks-det", "automations-det", "inbox-det", "notifications-det",
    "memory-det", "connections-det", "observability-det", "coach-det",
    "settings-det", "profile-det", "llm-det", "model-hub-det", "stt-det",
    "tools-det", "permissions-det", "tour-det",
]

def load(name):
    return json.load(open(f"{SCRIPTS}/{name}.json"))

master = load("master-det")
msteps = master["steps"]
# head = steps up to (not including) the first section-01 marker
i01 = next(i for i, s in enumerate(msteps) if s.get("label") == "section-01-nav-det")
head = msteps[:i01]

out = list(head)
dynamics = set()
for n, page in enumerate(ORDER, start=1):
    d = load(page)
    ps = d["steps"]
    # strip the leading onboarding-skip + waitGone preamble (2 steps)
    assert ps[0].get("testid") == "onboarding-skip" and ps[1].get("kind") == "waitGone", page
    body = ps[2:]
    dynamics.update(d.get("dynamicTestids", []))
    out.append({"kind": "screenshot", "label": f"section-{n:02d}-{page}"})
    out.append({"kind": "goto", "route": "dashboard"})
    out.append({"kind": "waitFor", "testid": "app-main"})
    out.extend(body)

regen = {}
for key, value in master.items():
    if key == "dynamicTestids":
        continue
    if key == "steps":
        if dynamics:
            regen["dynamicTestids"] = sorted(dynamics)
        regen["steps"] = out
        continue
    regen[key] = value

if "--write" in sys.argv:
    json.dump(regen, open(f"{SCRIPTS}/master-det.json", "w"), indent=2)
    open(f"{SCRIPTS}/master-det.json", "a").write("\n")
    print(f"WROTE master-det.json: {len(out)} steps")
    sys.exit(0)

same = regen == master
print(f"regen {len(out)} steps vs current {len(msteps)} steps; identical={same}")
if same:
    sys.exit(0)
for i, (a, b) in enumerate(zip(out, msteps)):
    if a != b:
        print(f"first diff at {i}:\n  regen: {a}\n  curr:  {b}")
        break
else:
    print("steps differ in length, or a top-level key differs "
          f"(regen keys {sorted(regen)}, current keys {sorted(master)})")
# A dry run that saw a drift and exited 0 was the defect: the derived file
# stayed 201 steps stale with the generator answering "fine" to every caller.
sys.exit(1)
