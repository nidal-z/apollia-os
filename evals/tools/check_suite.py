#!/usr/bin/env python3
# REASON: print-call: this module is a command-line entry point; print() is its
# output.
"""Guard the native-tool eval suites against the tree they claim to measure.

It answers four questions, and only questions a machine can settle:

  1. Do the suites parse? Not with a Python TOML reader, which would prove
     nothing about the harness, but with the product's own parser: an
     `apollia-os eval run` against a dead socket exits 1 when the suite is
     malformed and 2 once it has parsed and cannot reach a daemon.
  2. Does the ledger still cover the tree? The 18 names come from
     NATIVE_TOOL_NAMES in crates/apollia-tools/src/tool_registry.rs. A tool
     added there and nowhere here is a hole this guard reports.
  3. Is every assertion deterministic? An `llm_judge` assertion asks a model
     whether a model succeeded, and a task with only an `exit_code` assertion
     passes on any answer at all.
  4. Do the suites and the seed still agree? Every token the seed writes must
     be named by a suite; every `file_exists` path must sit in the seeded
     workspace. Otherwise the fixture drifts away from what is asserted and
     the suite fails for a reason that has nothing to do with the tool.

Exit codes:
  0  every check ran and found nothing
  1  at least one finding
  2  nothing was checked: a prerequisite is missing
"""

import importlib.util
import os
import re
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[1]
REGISTRY = REPO / "crates" / "apollia-tools" / "src" / "tool_registry.rs"
OFFLINE = HERE / "native-tools-offline.toml"
NETWORK = HERE / "native-tools-network.toml"
SEED = HERE / "seed_workspace.py"
DEFAULT_BIN = REPO / "target" / "debug" / "apollia-os"

# Tools whose task changes state. The seeded workspace is single use, so a
# second run of one of these measures the state the first run left behind.
STATEFUL = {
    "file_write",
    "file_edit",
    "notebook_edit",
    "permission_rule_add",
    "permission_rule_remove",
}

# Tools no task in this directory drives, with the reason. Each reason is a
# fact about the code, checked by hand and cited in README.md; this guard only
# holds the ledger closed, it does not re-prove the reason.
DECLARED_UNCOVERABLE = {
    "ask_user": (
        "the task-mode runner passes pending_user_inputs: None "
        "(crates/apollia-cli/src/commands/start/runner.rs), so the tool is not "
        "registered on the path `apollia-os eval run` drives"
    ),
}

WORKSPACE_ROOT = "/tmp/apollia-eval-tools/"


class Refusal(Exception):
    """Raised when a prerequisite is missing (exit 2)."""


def load_tomllib():
    """Return the stdlib TOML reader, or refuse."""
    try:
        import tomllib
    except ImportError as error:
        raise Refusal(f"no stdlib tomllib (Python 3.11+ required): {error}") from error
    return tomllib


def native_tool_names() -> list[str]:
    """Read NATIVE_TOOL_NAMES from the Rust registry, the tree's own list."""
    if not REGISTRY.is_file():
        raise Refusal(f"registry not found at {REGISTRY}")
    source = REGISTRY.read_text(encoding="utf-8")
    match = re.search(
        r"pub const NATIVE_TOOL_NAMES: &\[&str\] = &\[(.*?)\];", source, re.DOTALL
    )
    if match is None:
        raise Refusal("NATIVE_TOOL_NAMES not found in tool_registry.rs")
    names = re.findall(r'"([a-z0-9_]+)"', match.group(1))
    if not names:
        raise Refusal("NATIVE_TOOL_NAMES parsed as empty")
    return names


def seed_tokens() -> dict:
    """Load the token table from the seed script, without importing a package."""
    if not SEED.is_file():
        raise Refusal(f"seed script not found at {SEED}")
    spec = importlib.util.spec_from_file_location("apollia_eval_seed", SEED)
    if spec is None or spec.loader is None:
        raise Refusal("the seed script could not be loaded")
    module = importlib.util.module_from_spec(spec)
    # Byte-compiling the seed would drop a __pycache__ entry carrying this
    # checkout's absolute path into a tracked directory, which the prose guard
    # counts as a personal path.
    previous = sys.dont_write_bytecode
    sys.dont_write_bytecode = True
    try:
        spec.loader.exec_module(module)
    finally:
        sys.dont_write_bytecode = previous
    tokens = getattr(module, "TOKENS", None)
    if not isinstance(tokens, dict) or not tokens:
        raise Refusal("the seed script exposes no TOKENS table")
    return tokens


def binary() -> Path:
    """Return the apollia-os binary used as the parse oracle, or refuse."""
    override = os.environ.get("APOLLIA_OS_BIN")
    path = Path(override) if override else DEFAULT_BIN
    if not path.is_file():
        raise Refusal(
            f"no apollia-os binary at {path}; build it with "
            "`cargo build -p apollia-cli --bin apollia-os` or set APOLLIA_OS_BIN"
        )
    return path


def parses_with_product(bin_path: Path, suite: Path) -> str | None:
    """Return a finding when the product's own parser refuses `suite`."""
    dead_socket = "/tmp/apollia-eval-tools/no-daemon-here.sock"
    result = subprocess.run(
        [str(bin_path), "eval", "run", str(suite), "--socket", dead_socket],
        capture_output=True,
        text=True,
        check=False,
        timeout=120,
    )
    # 2 is "parsed, then no daemon"; 1 is "the suite itself was refused".
    if result.returncode == 2:
        return None
    if result.returncode == 1:
        first = (result.stdout + result.stderr).strip().splitlines()
        detail = first[0] if first else "no message"
        return f"{suite.name}: the product's parser refused it ({detail})"
    return (
        f"{suite.name}: the parse oracle returned {result.returncode}, "
        "which is neither a parse refusal (1) nor an unreachable daemon (2)"
    )


def check_ledger(names: list[str], covered: dict[str, Path]) -> list[str]:
    """Cross the registry list with what the suites and the ledger cover."""
    findings = []
    declared = set(DECLARED_UNCOVERABLE)
    for name in names:
        if name not in covered and name not in declared:
            findings.append(
                f"{name}: in NATIVE_TOOL_NAMES, driven by no task and not "
                "declared uncoverable"
            )
    known = set(names)
    for task_id, suite in covered.items():
        if task_id not in known:
            findings.append(
                f"{task_id} ({suite.name}): task id is not a native tool name"
            )
    for name in declared:
        if name in covered:
            findings.append(
                f"{name}: declared uncoverable but a task drives it; delete one "
                "of the two claims"
            )
        if name not in known:
            findings.append(
                f"{name}: declared uncoverable but absent from NATIVE_TOOL_NAMES"
            )
    return findings


def check_tasks(suite_path: Path, parsed: dict) -> tuple[list[str], dict[str, Path]]:
    """Check one suite's tasks and return its findings plus its coverage."""
    findings: list[str] = []
    covered: dict[str, Path] = {}
    tasks = parsed.get("tasks", [])
    if not tasks:
        findings.append(f"{suite_path.name}: no task at all")
    for task in tasks:
        task_id = task.get("id", "<unnamed>")
        if task_id in covered:
            findings.append(f"{task_id} ({suite_path.name}): duplicate task id")
        covered[task_id] = suite_path

        assertions = task.get("assertions", [])
        kinds = [assertion.get("type") for assertion in assertions]
        if "llm_judge" in kinds:
            findings.append(
                f"{task_id}: llm_judge assertion, which is not a deterministic check"
            )
        if not [kind for kind in kinds if kind in ("regex", "file_exists")]:
            findings.append(
                f"{task_id}: no regex or file_exists assertion, so any answer passes"
            )
        if not task.get("agent"):
            findings.append(f"{task_id}: no target agent, the run would need --agent")
        runs = task.get("runs", 3)
        if task_id in STATEFUL and runs != 1:
            findings.append(
                f"{task_id}: runs = {runs}, but it changes state and the seeded "
                "workspace is single use"
            )
        for assertion in assertions:
            if assertion.get("type") == "file_exists":
                path = assertion.get("path", "")
                if not path.startswith(WORKSPACE_ROOT):
                    findings.append(
                        f"{task_id}: file_exists path {path} is outside the seeded "
                        f"workspace {WORKSPACE_ROOT}"
                    )
    return findings, covered


def check_tokens(tokens: dict, suite_text: str) -> list[str]:
    """Every token the seed writes must be named by a suite."""
    unescaped = suite_text.replace("\\", "")
    return [
        f"token '{value}' ({key}) is seeded but never named by a suite"
        for key, value in tokens.items()
        if value not in unescaped
    ]


def main() -> int:
    """Run every check and report one of the three codes."""
    try:
        tomllib = load_tomllib()
        names = native_tool_names()
        tokens = seed_tokens()
        bin_path = binary()
        suites = []
        load_findings: list[str] = []
        for suite_path in (OFFLINE, NETWORK):
            if not suite_path.is_file():
                raise Refusal(f"suite not found at {suite_path}")
            try:
                parsed = tomllib.loads(suite_path.read_text("utf-8"))
            except tomllib.TOMLDecodeError as error:
                # A malformed suite is a finding, not a missing prerequisite:
                # the file is there and it is wrong. Reading on with an empty
                # task list keeps the rest of the checks running, and the
                # product parser below reports it a second time in its own
                # words.
                load_findings.append(f"{suite_path.name}: malformed TOML ({error})")
                parsed = {}
            suites.append((suite_path, parsed))
    except Refusal as refusal:
        print(f"NOT MEASURED: {refusal}")
        return 2

    findings: list[str] = list(load_findings)
    covered: dict[str, Path] = {}
    suite_text = ""
    for suite_path, parsed in suites:
        suite_text += suite_path.read_text(encoding="utf-8")
        suite_findings, suite_covered = check_tasks(suite_path, parsed)
        findings.extend(suite_findings)
        for task_id, owner in suite_covered.items():
            if task_id in covered:
                findings.append(f"{task_id}: driven by two suites")
            covered[task_id] = owner
        oracle = parses_with_product(bin_path, suite_path)
        if oracle is not None:
            findings.append(oracle)

    findings.extend(check_ledger(names, covered))
    findings.extend(check_tokens(tokens, suite_text))

    offline = sum(1 for owner in covered.values() if owner == OFFLINE)
    network = sum(1 for owner in covered.values() if owner == NETWORK)
    print(
        f"{len(names)} native tools: {offline} offline tasks, {network} network "
        f"tasks, {len(DECLARED_UNCOVERABLE)} declared uncoverable"
    )
    for name, reason in DECLARED_UNCOVERABLE.items():
        print(f"  uncoverable: {name} ({reason})")
    if findings:
        for finding in findings:
            print(f"FINDING: {finding}")
        return 1
    print("no finding")
    return 0


if __name__ == "__main__":
    sys.exit(main())
