#!/usr/bin/env python3
"""Generate master-det-scan.json: a fail-fast, continue-on-error copy of a
deterministic suite for a single enumeration run.

Purpose: after a large UI refactor, testids drift. A normal run waits the full
per-step timeout (default 15000ms) on every missing testid, so a run with 200+
broken anchors spends ~50 minutes just waiting. This scan caps every wait-like
step's timeout to a small value so a missing testid fails in seconds, while
steps that pass resolve instantly. One boot then yields the full batch of
failures in report.json (master-det already sets stopOnError=false, and the
runner catches each step, so the run never aborts on a missing testid).

The output is data only: no runner change, picked up at runtime by
`just desktop-dev-automation-seeded scripts/automation/master-det-scan.json`.
Feed the resulting report.json to tools/analyze_report.py to group failures by
section, then fix testids diff-first and rerun.

Usage:
    python3 scripts/automation/tools/make_scan.py            # from master-det, cap 3000ms
    python3 scripts/automation/tools/make_scan.py --src foo-det --cap 2500 --no-screens
"""
import json
import sys

SCRIPTS = "scripts/automation"

# step kinds whose timeout gates on locating an element (see runner.ts)
WAIT_KINDS = {
    "waitFor", "waitGone", "click", "expect", "captureText",
    "fill", "setChecked", "selectOption", "press",
}


def arg(flag, default):
    return sys.argv[sys.argv.index(flag) + 1] if flag in sys.argv else default


def main():
    src = arg("--src", "master-det")
    cap = int(arg("--cap", "3000"))
    drop_screens = "--no-screens" in sys.argv

    script = json.load(open(f"{SCRIPTS}/{src}.json"))
    out_steps = []
    capped = 0
    dropped = 0
    for step in script["steps"]:
        if drop_screens and step.get("kind") == "screenshot":
            dropped += 1
            continue
        if step.get("kind") in WAIT_KINDS:
            current = step.get("timeoutMs")
            if current is None or current > cap:
                step = dict(step)
                step["timeoutMs"] = cap
                capped += 1
        out_steps.append(step)

    scan = dict(script)
    scan["name"] = f"{script.get('name', src)} [SCAN cap={cap}ms]"
    scan["stopOnError"] = False
    scan["steps"] = out_steps

    dest = f"{SCRIPTS}/{src}-scan.json"
    json.dump(scan, open(dest, "w"), indent=2)
    open(dest, "a").write("\n")
    print(f"WROTE {dest}: {len(out_steps)} steps, capped {capped} timeouts to {cap}ms"
          + (f", dropped {dropped} screenshots" if drop_screens else ""))


if __name__ == "__main__":
    main()
