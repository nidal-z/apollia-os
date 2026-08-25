#!/usr/bin/env python3
"""Read the machine verdict of the last seeded desktop automation run.

`just desktop-dev-automation-seeded scripts/automation/master-det.json` drives
the real Tauri app through the 2400-step gestural corpus and writes its verdict
to `.apollia-automation/report.json` (`ok`, one entry per step). That file was
read by nobody: the runtime half of the corpus lens sat at ok=False for weeks,
10 red steps across 3 sections, while every chain stayed green because no chain
looked. This guard is the reader.

It answers three distinct things, each with its own exit code, because a
missing measurement must never read as a pass:

  0  the report is fresh (not older than the HEAD commit) and `ok` is true.
  1  the report is fresh and `ok` is false. The red sections are listed, each
     failure attributed to the nearest preceding `section-*` marker of the
     script that produced the run.
  2  nothing measured: the report is absent, unreadable, or finished before
     the HEAD commit was made, so it states nothing about the current tree.
     Rerun the recipe above to produce a verdict.

There is no CI frontier for this guard: no hosted runner drives WKWebView, so
the run stays a local act. The frontiers that launch this reader are the
`desktop-automation-verdict` recipe of the justfile and the heavy-guards table
of `scripts/worktree_verdicts.py`.

Usage:
    python3 scripts/check_automation_report.py [report.json] [script.json]
"""

import argparse
import json
import subprocess
import sys
from datetime import datetime
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
DEFAULT_REPORT = REPO_ROOT / ".apollia-automation/report.json"
DEFAULT_SCRIPT = REPO_ROOT / "scripts/automation/master-det.json"


def head_committed_at() -> datetime:
    """Committer date of HEAD, timezone-aware."""
    out = subprocess.run(
        ["git", "-C", str(REPO_ROOT), "log", "-1", "--format=%cI"],
        capture_output=True,
        text=True,
        check=True,
    ).stdout.strip()
    return datetime.fromisoformat(out)


def parse_when(iso: str) -> datetime:
    """The report's own ISO-8601 timestamp (runner writes a trailing Z)."""
    return datetime.fromisoformat(iso.replace("Z", "+00:00"))


def sections_of(script_path: Path) -> dict[int, str]:
    """Step index to section label, from the script that produced the run.

    The report carries flat step indexes; the script marks its sections with
    `screenshot` steps labelled `section-*`. Same attribution as
    `scripts/automation/tools/analyze_report.py`, so the two tools name the
    same section for the same failure.
    """
    steps = json.loads(script_path.read_text(encoding="utf-8"))["steps"]
    current = "(preamble)"
    mapping: dict[int, str] = {}
    for index, step in enumerate(steps):
        if step.get("kind") == "screenshot":
            label = step.get("label", "")
            if label.startswith("section-"):
                current = label
        mapping[index] = current
    return mapping


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument(
        "report", nargs="?", type=Path, default=DEFAULT_REPORT,
        help="report.json written by the seeded automation run",
    )
    parser.add_argument(
        "script", nargs="?", type=Path, default=DEFAULT_SCRIPT,
        help="automation script the run replayed",
    )
    args = parser.parse_args()
    report_path = args.report
    script_path = args.script

    if not report_path.is_file():
        print(
            f"NOTHING MEASURED: {report_path} does not exist. The gestural "
            "corpus only renders a runtime verdict after a run of the real "
            "app: just desktop-dev-automation-seeded "
            "scripts/automation/master-det.json"
        )
        return 2

    try:
        report = json.loads(report_path.read_text(encoding="utf-8"))
        finished_at = parse_when(report["finishedAt"])
        ok = bool(report["ok"])
        steps = report["steps"]
    except (json.JSONDecodeError, KeyError, ValueError, TypeError) as err:
        print(f"NOTHING MEASURED: {report_path} is not a readable report: {err}")
        return 2

    head_at = head_committed_at()
    if finished_at < head_at:
        print(
            f"NOTHING MEASURED: the report finished at {report['finishedAt']}, "
            f"before the HEAD commit ({head_at.isoformat()}). It measured an "
            "older tree and states nothing about this one. Rerun: just "
            "desktop-dev-automation-seeded scripts/automation/master-det.json"
        )
        return 2

    failed = [step for step in steps if not step.get("ok", True)]
    if not ok or failed:
        by_section: dict[str, int] = {}
        if script_path.is_file():
            mapping = sections_of(script_path)
            for step in failed:
                section = mapping.get(step.get("index", -1), "?")
                by_section[section] = by_section.get(section, 0) + 1
        else:
            by_section["(script not found, sections unattributed)"] = len(failed)
        print(
            f"RED: {report['script']}: ok={ok}, {len(failed)} failed step(s) "
            f"out of {len(steps)}, in {len(by_section)} section(s):"
        )
        for section in sorted(by_section):
            print(f"  {section}: {by_section[section]} failed")
        return 1

    print(
        f"ok: {report['script']}: {len(steps)} steps, 0 failed, "
        f"finished {report['finishedAt']} (HEAD committed {head_at.isoformat()})"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
