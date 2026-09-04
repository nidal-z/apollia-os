#!/usr/bin/env python3
# REASON: print-call: this module is a command-line entry point; print() is its
# output.
"""Apply, by hand, the effects a run would leave, so the oracle can be proven.

`verify_effects.py` is a measuring instrument, and an instrument nobody has
ever seen fail measures nothing. Running the real suite needs a daemon and a
model; this script needs neither. It writes into the seeded workspace exactly
what a perfect run would leave (`perfect`), or a plausible near miss
(`sloppy`: right file, wrong content), and emits the matching results JSONL.

It writes nowhere but /tmp/apollia-eval-tools, and it is not part of running
the suite: it exists to prove the oracle answers 0 and 1 for the right
reasons.

Usage:
    simulate_run.py perfect|sloppy [--jsonl PATH]

Exit codes:
  0  the simulated effects were applied
  2  nothing was applied: there is no seeded workspace
"""

import json
import sqlite3
import sys
from pathlib import Path

ROOT = Path("/tmp/apollia-eval-tools")
HOME = ROOT / "home"
FIXTURES = HOME / "eval-tools"
GOVERNANCE = HOME / ".apollia" / "governance.db"

STATEFUL_TASKS = (
    "file_write",
    "file_edit",
    "notebook_edit",
    "permission_rule_add",
    "permission_rule_remove",
)


def apply_effects(mode: str) -> None:
    """Write the effects of a `mode` run into the seeded workspace."""
    written = FIXTURES / "out" / "file-write.txt"
    written.parent.mkdir(parents=True, exist_ok=True)
    written.write_text(
        "APOLLIA-WRITE-7B3D50C1\n" if mode == "perfect" else "almost right\n",
        encoding="utf-8",
    )

    config = FIXTURES / "edit" / "config.txt"
    replacement = "APPROVED-2E7B" if mode == "perfect" else "APPROVED-WRONG"
    config.write_text(
        config.read_text(encoding="utf-8").replace("PENDING-9C4A", replacement),
        encoding="utf-8",
    )

    notebook_path = FIXTURES / "nb" / "edit.ipynb"
    notebook = json.loads(notebook_path.read_text(encoding="utf-8"))
    notebook["cells"][1]["source"] = ["# APOLLIA-NBEDIT-0C82"]
    if mode == "sloppy":
        # The near miss that a `file_exists` assertion cannot see: the cell is
        # replaced, and the rest of the notebook is dropped on the way.
        notebook["cells"] = notebook["cells"][:2]
    notebook_path.write_text(json.dumps(notebook, indent=1), encoding="utf-8")

    with sqlite3.connect(GOVERNANCE) as conn:
        conn.execute(
            "INSERT INTO permission_rules (tool_name, arg_prefix, action, created_at,"
            " created_by, scope) VALUES (?, NULL, ?, 0, ?, 'global')",
            (
                "eval-probe-add",
                "deny" if mode == "perfect" else "allow",
                "eval-tools-probe",
            ),
        )
        conn.execute("DELETE FROM permission_rules WHERE tool_name = 'eval-probe-remove'")
        if mode == "sloppy":
            conn.execute("DELETE FROM permission_rules WHERE tool_name = 'eval-probe-list'")


def write_jsonl(path: Path) -> None:
    """Emit one passing record per stateful task, in the harness's shape."""
    lines = [
        json.dumps(
            {
                "task_id": task_id,
                "run_index": 0,
                "passed": True,
                "failure_reason": None,
                "exit_code": 0,
                "steps": 3,
                "tool_calls": 1,
                "wall_clock_ms": 1000,
                "cost_usd": 0.0,
            }
        )
        for task_id in STATEFUL_TASKS
    ]
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")


def main() -> int:
    """Apply the requested simulation, or refuse when there is nothing to write."""
    mode = sys.argv[1] if len(sys.argv) > 1 else ""
    if mode not in ("perfect", "sloppy"):
        print("NOT MEASURED: usage: simulate_run.py perfect|sloppy [--jsonl PATH]")
        return 2
    if not FIXTURES.is_dir() or not GOVERNANCE.is_file():
        print(f"NOT MEASURED: no seeded workspace at {ROOT}; run seed_workspace.py")
        return 2

    jsonl = None
    if "--jsonl" in sys.argv:
        jsonl = Path(sys.argv[sys.argv.index("--jsonl") + 1])

    apply_effects(mode)
    if jsonl is not None:
        write_jsonl(jsonl)
        print(f"simulated a {mode} run, results written to {jsonl}")
    else:
        print(f"simulated a {mode} run")
    return 0


if __name__ == "__main__":
    sys.exit(main())
