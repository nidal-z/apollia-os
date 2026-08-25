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

Only the kinds whose `timeoutMs` gates on locating an element are capped:
waitFor, waitGone, click, fill, selectOption, press (see runner.ts). The
runner reads no timeout on expect, captureText or setChecked, so a cap there
is an inert key, and awaitTurn's timeout bounds a whole agent turn rather
than a lookup, so capping it would abort legitimate turns.

The output is data only: no runner change, picked up at runtime by
`just desktop-dev-automation-seeded scripts/automation/master-det-scan.json`.
Feed the resulting report.json to tools/analyze_report.py to group failures by
section, then fix testids diff-first and rerun.

Usage:
    python3 scripts/automation/tools/make_scan.py            # from master-det, cap 3000ms
    python3 scripts/automation/tools/make_scan.py --src foo-det --cap 2500 --no-screens
"""
import argparse
import json

SCRIPTS = "scripts/automation"

# step kinds whose timeoutMs the runner reads to locate an element
WAIT_KINDS = {"waitFor", "waitGone", "click", "fill", "selectOption", "press"}


def main():
    parser = argparse.ArgumentParser(
        description="fail-fast scan copy of a deterministic suite"
    )
    parser.add_argument("--src", default="master-det",
                        help="source script under scripts/automation (default master-det)")
    parser.add_argument("--cap", type=int, default=3000,
                        help="timeout ceiling in ms for wait-like steps (default 3000)")
    parser.add_argument("--no-screens", action="store_true",
                        help="drop screenshot steps")
    args = parser.parse_args()

    script = json.load(open(f"{SCRIPTS}/{args.src}.json"))
    out_steps = []
    capped = 0
    dropped = 0
    for step in script["steps"]:
        if args.no_screens and step.get("kind") == "screenshot":
            dropped += 1
            continue
        if step.get("kind") in WAIT_KINDS:
            current = step.get("timeoutMs")
            if current is None or current > args.cap:
                step = dict(step)
                step["timeoutMs"] = args.cap
                capped += 1
        out_steps.append(step)

    scan = dict(script)
    scan["name"] = f"{script.get('name', args.src)} [SCAN cap={args.cap}ms]"
    scan["stopOnError"] = False
    scan["steps"] = out_steps

    dest = f"{SCRIPTS}/{args.src}-scan.json"
    json.dump(scan, open(dest, "w"), indent=2)
    open(dest, "a").write("\n")
    print(f"WROTE {dest}: {len(out_steps)} steps, capped {capped} timeouts to {args.cap}ms"
          + (f", dropped {dropped} screenshots" if args.no_screens else ""))


if __name__ == "__main__":
    main()
