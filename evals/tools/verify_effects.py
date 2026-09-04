#!/usr/bin/env python3
# REASON: print-call: this module is a command-line entry point; print() is its
# output.
"""Read the workspace back after a run and say what each tool actually did.

The harness can assert three things: an exit code, that a path exists, and a
regex over the answer. None of those reads a file's content, so a run where
`file_edit` wrote the wrong text into the right file, or where `notebook_edit`
flattened a notebook while replacing a cell, passes the suite. This oracle
closes that gap for the five state-changing tools by reading the workspace and
the governance database directly, and it never asks a model anything.

Usage:
    verify_effects.py [results.jsonl]

The JSONL, written by `apollia-os eval run`, is what separates "the task ran
and left nothing" from "the task never ran". Without it, an absent effect is
reported as not measured rather than as a defect.

Exit codes:
  0  every effect this oracle can read was there and correct
  1  at least one effect is wrong, or is missing after its task ran
  2  nothing was measured: no workspace, or no effect could be judged
"""

import json
import sqlite3
import sys
from pathlib import Path

ROOT = Path("/tmp/apollia-eval-tools")
HOME = ROOT / "home"
FIXTURES = HOME / "eval-tools"
GOVERNANCE = HOME / ".apollia" / "governance.db"

OK = "OK"
DEFECT = "DEFECT"
UNMEASURED = "NOT MEASURED"


def read_json(path: Path):
    """Return parsed JSON, or None when the file is unreadable."""
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return None


def load_runs(jsonl: Path | None) -> dict:
    """Group the per-run records of a results JSONL by task id."""
    if jsonl is None or not jsonl.is_file():
        return {}
    runs: dict[str, list] = {}
    for line in jsonl.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if not line:
            continue
        try:
            record = json.loads(line)
        except json.JSONDecodeError:
            continue
        runs.setdefault(record.get("task_id", "<unnamed>"), []).append(record)
    return runs


def ran(runs: dict, task_id: str) -> bool:
    """Whether the harness drove this task at all."""
    return bool(runs.get(task_id))


def timed_out(runs: dict, task_id: str) -> bool:
    """Whether every run of this task ended in a timeout or an undriven run.

    A task the runtime never answered measured nothing; reporting it as a
    defect of the tool would be a fabricated verdict.
    """
    records = runs.get(task_id, [])
    if not records:
        return False
    for record in records:
        reason = (record.get("failure_reason") or "").lower()
        if "timed out" not in reason and record.get("exit_code") != -1:
            return False
    return True


def absent(runs: dict, task_id: str, detail: str) -> tuple[str, str]:
    """Classify a missing effect, given what the JSONL says about the task."""
    if timed_out(runs, task_id):
        return UNMEASURED, f"{detail}; the task timed out or was never driven"
    if ran(runs, task_id):
        return DEFECT, f"{detail}; the task ran and left no effect"
    return UNMEASURED, f"{detail}; no results JSONL says this task ever ran"


def check_file_write(runs: dict) -> tuple[str, str]:
    """The written file must exist and hold exactly the expected line."""
    target = FIXTURES / "out" / "file-write.txt"
    if not target.is_file():
        return absent(runs, "file_write", f"{target} does not exist")
    content = target.read_text(encoding="utf-8").strip()
    if content != "APOLLIA-WRITE-7B3D50C1":
        return DEFECT, f"{target} holds {content!r}, not the expected single line"
    return OK, "the file holds exactly the expected line"


def check_file_edit(runs: dict) -> tuple[str, str]:
    """The edit must replace one token and leave the rest of the file alone."""
    target = FIXTURES / "edit" / "config.txt"
    if not target.is_file():
        return UNMEASURED, f"{target} is missing; re-seed the workspace"
    lines = target.read_text(encoding="utf-8").splitlines()
    if "status = PENDING-9C4A" in lines:
        return absent(runs, "file_edit", "config.txt still holds the old token")
    if "status = APPROVED-2E7B" not in lines:
        return DEFECT, f"config.txt holds {lines!r}, not the expected replacement"
    if lines != ["name = eval-tools", "status = APPROVED-2E7B", "retries = 3"]:
        return DEFECT, f"config.txt was rewritten beyond the replacement: {lines!r}"
    return OK, "one token replaced, every other line untouched"


def check_notebook_edit(runs: dict) -> tuple[str, str]:
    """The notebook must keep its shape and carry the new cell source."""
    target = FIXTURES / "nb" / "edit.ipynb"
    parsed = read_json(target)
    if parsed is None:
        return DEFECT if target.exists() else UNMEASURED, (
            f"{target} is not readable as JSON"
        )
    cells = parsed.get("cells", [])
    source = "".join(cells[1].get("source", [])) if len(cells) > 1 else ""
    if "# APOLLIA-NBEDIT-0C82" not in source:
        return absent(runs, "notebook_edit", "cell 1 does not carry the new source")
    if parsed.get("nbformat") != 4 or len(cells) != 3:
        return DEFECT, (
            f"the notebook is now nbformat {parsed.get('nbformat')} with "
            f"{len(cells)} cells, expected v4 with 3"
        )
    if "value = 1" not in "".join(cells[0].get("source", [])):
        return DEFECT, "cell 0 was altered by the edit"
    return OK, "cell 1 replaced, notebook shape preserved"


def governance_rules(tool_name: str) -> list | None:
    """Return the rules stored for `tool_name`, or None when unreadable."""
    if not GOVERNANCE.is_file() or GOVERNANCE.stat().st_size == 0:
        return None
    query = "SELECT id, action, scope FROM permission_rules WHERE tool_name = ?"
    # A read-only connection is the right default for an oracle, but SQLite
    # refuses one on a database the runtime left in WAL mode: recovering the
    # -wal file needs write access to its sidecars. Falling back to a normal
    # connection (which issues no write) is the difference between reading the
    # rules and reporting "unreadable" on a database that is perfectly fine.
    for uri, kwargs in (
        (f"file:{GOVERNANCE}?mode=ro", {"uri": True}),
        (str(GOVERNANCE), {}),
    ):
        try:
            with sqlite3.connect(uri, **kwargs) as conn:
                return conn.execute(query, (tool_name,)).fetchall()
        except sqlite3.Error:
            continue
    return None


def check_permission_add(runs: dict) -> tuple[str, str]:
    """The added rule must exist in governance.db with the asked-for action."""
    rules = governance_rules("eval-probe-add")
    if rules is None:
        return UNMEASURED, "governance.db is missing or unreadable"
    if not rules:
        return absent(runs, "permission_rule_add", "no rule for eval-probe-add")
    actions = {row[1] for row in rules}
    if actions != {"deny"}:
        return DEFECT, f"eval-probe-add rules carry actions {actions}, expected deny"
    return OK, f"{len(rules)} deny rule(s) persisted for eval-probe-add"


def check_permission_remove(runs: dict) -> tuple[str, str]:
    """The seeded rule must be gone, and nothing else with it."""
    removed = governance_rules("eval-probe-remove")
    kept = governance_rules("eval-probe-list")
    if removed is None or kept is None:
        return UNMEASURED, "governance.db is missing or unreadable"
    if len(removed) == 1:
        return absent(runs, "permission_rule_remove", "the seeded rule is still there")
    if removed:
        return DEFECT, f"{len(removed)} rules for eval-probe-remove, expected 0 or 1"
    if len(kept) != 3:
        return DEFECT, (
            f"the removal also took the eval-probe-list rules: {len(kept)} left of 3"
        )
    return OK, "the seeded rule is gone and the three list rules are intact"


CHECKS = {
    "file_write": check_file_write,
    "file_edit": check_file_edit,
    "notebook_edit": check_notebook_edit,
    "permission_rule_add": check_permission_add,
    "permission_rule_remove": check_permission_remove,
}

# Tools this oracle cannot judge from disk: they leave no trace, so the
# harness's own regex assertion is their only judge. Listed so a reader is
# never left believing the oracle covered them.
ASSERTION_ONLY = (
    "file_read",
    "file_list",
    "file_glob",
    "file_grep",
    "notebook_read",
    "memory_search",
    "bash_executor",
    "python_executor",
    "http_fetch",
    "permission_rule_list",
    "web_search",
    "web_read",
)


def python_note() -> str | None:
    """Say whether this host could have run python_executor at all."""
    state = read_json(ROOT / "preconditions.json")
    if state is None:
        return None
    if not state.get("python3") or not state.get("python3_venv_module"):
        return (
            "python_executor: no usable python3 on this host, so a failure of "
            "that task is a missing interpreter, not a broken tool"
        )
    return None


def main() -> int:
    """Read every effect back and report one of the three codes."""
    if not FIXTURES.is_dir():
        print(f"NOT MEASURED: no seeded workspace at {FIXTURES}; run seed_workspace.py")
        return 2

    jsonl = Path(sys.argv[1]) if len(sys.argv) > 1 else None
    if jsonl is not None and not jsonl.is_file():
        print(f"NOT MEASURED: no results JSONL at {jsonl}")
        return 2
    runs = load_runs(jsonl)

    verdicts = {}
    for task_id, check in CHECKS.items():
        verdicts[task_id] = check(runs)

    for task_id, (verdict, detail) in verdicts.items():
        print(f"{verdict}: {task_id}: {detail}")

    for task_id in ASSERTION_ONLY:
        records = runs.get(task_id, [])
        if records:
            passed = sum(1 for record in records if record.get("passed"))
            print(f"assertion-only: {task_id}: {passed}/{len(records)} runs passed")
        else:
            print(f"assertion-only: {task_id}: no run recorded, nothing to report")

    note = python_note()
    if note is not None:
        print(f"note: {note}")

    defects = [task for task, (verdict, _) in verdicts.items() if verdict == DEFECT]
    unmeasured = [task for task, (verdict, _) in verdicts.items() if verdict == UNMEASURED]
    print(
        f"{len(verdicts) - len(defects) - len(unmeasured)}/{len(verdicts)} effects "
        f"verified, {len(defects)} wrong, {len(unmeasured)} not measured"
    )
    if defects:
        return 1
    if unmeasured:
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main())
