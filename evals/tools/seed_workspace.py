#!/usr/bin/env python3
# REASON: print-call: this module is a command-line entry point; print() is its
# output.
"""Build the throwaway workspace the native-tool eval suites measure against.

Everything lives under /tmp/apollia-eval-tools. The real ~/.apollia is never
opened: every command this script runs is given HOME=<root>/home. The path is a
fixed literal rather than a mkdtemp because the harness's `file_exists`
assertion takes a literal path and does not expand anything.

The script is not only a copier. Each fixture it writes for a stateful surface
(memory, permissions) is read back through the product's own read path, so a
workspace that would silently measure nothing is refused here rather than
reported green three hundred seconds later.

Exit codes, the same three everywhere in this directory:
  0  the workspace is built and every read-back agreed with what was written
  1  a defect: a step ran and produced the wrong result
  2  nothing was measured: a prerequisite is missing, so no verdict is possible
"""

import json
import os
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path("/tmp/apollia-eval-tools")
HOME = ROOT / "home"
APOLLIA = HOME / ".apollia"
FIXTURES = HOME / "eval-tools"
MEMORY_DIR = APOLLIA / "memory"
MEMORY_NAMESPACE = "eval-tools-probe"
AGENT_NAME = "eval-tools-probe"

# The tokens the suites assert on. They exist nowhere else on the machine, so a
# model that does not call the tool cannot produce them.
TOKENS = {
    "read": "APOLLIA-READ-4F1C9A2E",
    "write": "APOLLIA-WRITE-7B3D50C1",
    "edit_before": "PENDING-9C4A",
    "edit_after": "APPROVED-2E7B",
    "list": "6D2F",
    "glob": "needle-83F1",
    "grep": "MARKER-GREP-D91A0C55",
    "notebook_read": "NB-TOKEN-5A7E31",
    "notebook_edit": "# APOLLIA-NBEDIT-0C82",
    "memory": "MEMTOKEN-B4E90D7A",
    "bash_lines": "137",
    "python_result": "139776",
}

REPO = Path(__file__).resolve().parents[2]
DEFAULT_BIN = REPO / "target" / "debug" / "apollia-os"


class Refusal(Exception):
    """Raised when the workspace cannot be built at all (exit 2)."""


class Defect(Exception):
    """Raised when a step ran but produced the wrong result (exit 1)."""


def binary() -> Path:
    """Return the apollia-os binary to drive, or refuse when there is none."""
    override = os.environ.get("APOLLIA_OS_BIN")
    path = Path(override) if override else DEFAULT_BIN
    if not path.is_file():
        raise Refusal(
            f"no apollia-os binary at {path}; build it with "
            "`cargo build -p apollia-cli --bin apollia-os` or set APOLLIA_OS_BIN"
        )
    return path


def run(argv: list[str], env_home: Path) -> subprocess.CompletedProcess:
    """Run a command with a throwaway HOME, capturing both streams."""
    env = dict(os.environ)
    env["HOME"] = str(env_home)
    env.pop("APOLLIA_HOME", None)
    return subprocess.run(
        argv, env=env, capture_output=True, text=True, check=False, timeout=180
    )


def guard_target() -> None:
    """Refuse to build anywhere but a temporary directory."""
    resolved = ROOT.resolve()
    if not (str(resolved).startswith("/tmp/") or str(resolved).startswith("/private/tmp/")):
        raise Refusal(f"refusing to build outside /tmp: {resolved}")
    real_home = Path(os.path.expanduser("~")).resolve()
    if resolved == real_home or real_home in resolved.parents:
        raise Refusal(f"refusing to build inside the real home: {resolved}")


def write(path: Path, content: str) -> None:
    """Write a fixture file, creating its parents."""
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content, encoding="utf-8")


def notebook(cells: list[dict]) -> str:
    """Serialise a minimal nbformat v4 notebook."""
    return json.dumps(
        {
            "cells": cells,
            "metadata": {},
            "nbformat": 4,
            "nbformat_minor": 5,
        },
        indent=1,
    )


def cell(kind: str, source: str) -> dict:
    """Build one nbformat v4 cell."""
    base = {"cell_type": kind, "metadata": {}, "source": source.splitlines(keepends=True)}
    if kind == "code":
        base["execution_count"] = None
        base["outputs"] = []
    return base


def build_files() -> None:
    """Write every filesystem fixture the offline suite reads."""
    write(FIXTURES / "read" / "secret.txt", f"token: {TOKENS['read']}\n")

    for name in ("alpha", "bravo", "charlie"):
        write(FIXTURES / "list" / f"{name}-{TOKENS['list']}.txt", f"{name}\n")

    write(FIXTURES / "glob" / "a" / f"{TOKENS['glob']}.log", "match one\n")
    write(FIXTURES / "glob" / "b" / "c" / f"{TOKENS['glob']}.log", "match two\n")
    write(FIXTURES / "glob" / "d" / f"{TOKENS['glob']}.txt", "decoy, wrong extension\n")
    write(FIXTURES / "glob" / "a" / "unrelated.log.bak", "decoy, wrong suffix\n")

    for index in (1, 2, 4):
        write(
            FIXTURES / "grep" / f"notes-{index}.md",
            f"# notes {index}\n\nnothing to find here\n",
        )
    write(
        FIXTURES / "grep" / "notes-3.md",
        "# notes 3\n\nheader\n" + TOKENS["grep"] + "\ntail\n",
    )

    write(
        FIXTURES / "edit" / "config.txt",
        "name = eval-tools\n"
        f"status = {TOKENS['edit_before']}\n"
        "retries = 3\n",
    )

    write(
        FIXTURES / "bash" / "lines.txt",
        "".join(f"line {i:03d}\n" for i in range(1, 138)),
    )

    analysis = notebook(
        [
            cell("code", "value = 1\n"),
            cell("markdown", "## Intermediate cell\n"),
            cell("markdown", f"token: {TOKENS['notebook_read']}\n"),
        ]
    )
    write(FIXTURES / "nb" / "analysis.ipynb", analysis)
    write(
        FIXTURES / "nb" / "edit.ipynb",
        notebook(
            [
                cell("code", "value = 1\n"),
                cell("code", "# placeholder to replace\n"),
                cell("markdown", "## Trailing cell\n"),
            ]
        ),
    )


def check_files() -> None:
    """Read the filesystem fixtures back and refuse a workspace that lies."""
    secret = (FIXTURES / "read" / "secret.txt").read_text(encoding="utf-8")
    if TOKENS["read"] not in secret:
        raise Defect("the read fixture does not carry its token")

    line_count = len(
        (FIXTURES / "bash" / "lines.txt").read_text(encoding="utf-8").splitlines()
    )
    if line_count != int(TOKENS["bash_lines"]):
        raise Defect(f"the bash fixture has {line_count} lines, expected 137")

    grep_hits = [
        path
        for path in FIXTURES.rglob("*")
        if path.is_file() and TOKENS["grep"] in path.read_text(encoding="utf-8", errors="ignore")
    ]
    if len(grep_hits) != 1:
        raise Defect(f"the grep token appears in {len(grep_hits)} files, expected 1")
    hit_lines = (
        FIXTURES / "grep" / "notes-3.md"
    ).read_text(encoding="utf-8").splitlines()
    if hit_lines.index(TOKENS["grep"]) + 1 != 4:
        raise Defect("the grep token is not on line 4 of notes-3.md")

    logs = sorted(str(p.relative_to(FIXTURES)) for p in FIXTURES.rglob("glob/**/*.log"))
    if logs != ["glob/a/needle-83F1.log", "glob/b/c/needle-83F1.log"]:
        raise Defect(f"the glob fixture matches {logs}, expected exactly two logs")

    for name in ("analysis.ipynb", "edit.ipynb"):
        parsed = json.loads((FIXTURES / "nb" / name).read_text(encoding="utf-8"))
        if parsed["nbformat"] != 4 or len(parsed["cells"]) != 3:
            raise Defect(f"{name} is not a three-cell nbformat v4 notebook")


def seed_memory(bin_path: Path) -> None:
    """Seed one episodic memory and prove the product's search path finds it."""
    MEMORY_DIR.mkdir(parents=True, exist_ok=True)
    entry_id = "00000000-0000-4000-8000-eva10000001"
    content = f"Eval fixture entry, token {TOKENS['memory']}, for memory_search."
    export = {
        "format_version": 1,
        "namespace": MEMORY_NAMESPACE,
        "exported_at": "2026-01-01T00:00:00Z",
        "episodic": [
            {
                "id": entry_id,
                "namespace": MEMORY_NAMESPACE,
                "agent_id": AGENT_NAME,
                "task_id": None,
                "content": content,
                "importance": 0.9,
                "created_at": "2026-01-01T00:00:00Z",
                "expires_at": None,
                "metadata": "{}",
            }
        ],
        "semantic": [],
        "procedural": [],
    }
    export_path = ROOT / "memory-seed.apollia-memory"
    write(export_path, json.dumps(export, indent=2))

    imported = run(
        [
            str(bin_path), "memory", "import",
            "--namespace", MEMORY_NAMESPACE,
            "--input", str(export_path),
            "--replace",
            "--data-dir", str(MEMORY_DIR),
        ],
        HOME,
    )
    if imported.returncode != 0:
        raise Defect(f"`memory import` failed: {imported.stderr.strip()}")

    # `import_namespace` inserts the row but not its FTS5 index entry, and
    # `memory_search` reads the index, not the table. The fixture therefore
    # writes the index row the way the episodic writer does
    # (crates/apollia-memory/src/episodic.rs), the same technique
    # tests/cli/seed/files/memory/build.sh uses for its own fixtures.
    sqlite = shutil.which("sqlite3")
    if sqlite is None:
        raise Refusal("sqlite3 is not on PATH; the memory fixture cannot be indexed")
    db = MEMORY_DIR / f"{MEMORY_NAMESPACE}.db"
    statement = (
        "INSERT INTO memory_fts (content, source_table, source_id) VALUES "
        f"('{content}', 'episodic', '{entry_id}');"
    )
    indexed = subprocess.run(
        [sqlite, str(db)], input=statement, capture_output=True, text=True, check=False
    )
    if indexed.returncode != 0:
        raise Defect(f"indexing the memory fixture failed: {indexed.stderr.strip()}")

    found = run(
        [
            str(bin_path), "memory", "search", MEMORY_NAMESPACE, "MEMTOKEN",
            "--data-dir", str(MEMORY_DIR),
        ],
        HOME,
    )
    if found.returncode != 0 or TOKENS["memory"] not in found.stdout:
        raise Defect(
            "the seeded memory is invisible to `memory search`, so memory_search "
            f"would measure nothing (exit {found.returncode})"
        )


def seed_permissions(bin_path: Path) -> None:
    """Seed the permission rules the three governance tasks depend on."""
    wanted = [("eval-probe-list", 3), ("eval-probe-remove", 1)]
    for tool, count in wanted:
        for index in range(count):
            added = run(
                [
                    str(bin_path), "permissions", "add",
                    "--tool", tool,
                    "--prefix", f"/tmp/apollia-eval-tools/probe-{index}",
                    "--action", "deny",
                    "--scope", "global",
                ],
                HOME,
            )
            if added.returncode != 0:
                raise Defect(
                    f"`permissions add` failed for {tool}: {added.stderr.strip()}"
                )

    listed = run([str(bin_path), "--json", "permissions", "list"], HOME)
    if listed.returncode != 0:
        raise Defect(f"`permissions list` failed: {listed.stderr.strip()}")
    for tool, count in wanted:
        seen = listed.stdout.count(f'"{tool}"')
        if seen < count:
            raise Defect(
                f"governance.db holds {seen} rules for {tool}, expected {count}"
            )


def install_agent(bin_path: Path) -> None:
    """Install the fixture agent into the throwaway HOME."""
    source = Path(__file__).resolve().parent / "agent"
    installed = run([str(bin_path), "agent", "install", str(source), "--skip-tests"], HOME)
    if installed.returncode != 0:
        raise Refusal(
            "the fixture agent could not be installed, so no task can be driven: "
            f"{(installed.stderr or installed.stdout).strip()[:600]}"
        )


def preconditions(bin_path: Path) -> dict:
    """Record what this host can and cannot measure, for the oracle to read."""
    python3 = shutil.which("python3")
    venv_ok = False
    if python3 is not None:
        probe = subprocess.run(
            [python3, "-c", "import venv"], capture_output=True, text=True, check=False
        )
        venv_ok = probe.returncode == 0
    return {
        "apollia_os_bin": str(bin_path),
        "sqlite3": shutil.which("sqlite3"),
        "python3": python3,
        "python3_venv_module": venv_ok,
        "workspace_root": str(ROOT),
        "home": str(HOME),
        "memory_namespace": MEMORY_NAMESPACE,
        "agent": AGENT_NAME,
        "tokens": TOKENS,
    }


def main() -> int:
    """Build the workspace, read it back, and report one of the three codes."""
    try:
        guard_target()
        bin_path = binary()
        if ROOT.exists():
            shutil.rmtree(ROOT)
        APOLLIA.mkdir(parents=True)
        build_files()
        check_files()
        seed_memory(bin_path)
        seed_permissions(bin_path)
        install_agent(bin_path)
        state = preconditions(bin_path)
        write(ROOT / "preconditions.json", json.dumps(state, indent=2))
    except Refusal as refusal:
        print(f"NOT MEASURED: {refusal}")
        return 2
    except Defect as defect:
        print(f"DEFECT: {defect}")
        return 1
    except (OSError, subprocess.SubprocessError, json.JSONDecodeError) as error:
        print(f"NOT MEASURED: the workspace could not be built: {error}")
        return 2
    print(f"seeded {ROOT}")
    print(f"  HOME for every run: {HOME}")
    print("  start the daemon with that HOME, then run the offline suite")
    return 0


if __name__ == "__main__":
    sys.exit(main())
