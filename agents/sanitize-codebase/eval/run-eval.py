"""Eval suite runner for sanitize-codebase.

Reads cases.jsonl, invokes the agent via the Apollia CLI in a subprocess
(`apollia agent run sanitize-codebase-director --skill <id> --input <json>`),
collects the JSON result, validates against per-case expectations, and
prints a one-line summary per case plus an aggregate report.

This is intentionally a thin harness: we do NOT spin up an LLM in
isolation. The eval is meant to be executed against a running daemon
where the agent is installed and the local LLM is configured.

Usage:
    cd agents/sanitize-codebase
    python eval/run-eval.py --runs 3
"""

from __future__ import annotations

import argparse
import json
import statistics
import subprocess
import sys
import time
from pathlib import Path

EVAL_DIR = Path(__file__).resolve().parent
CASES_PATH = EVAL_DIR / "cases.jsonl"

DEFAULT_SUCCESS_THRESHOLD = 0.80
DEFAULT_RUNS = 3


def load_cases() -> list[dict]:
    cases: list[dict] = []
    with CASES_PATH.open() as fh:
        for line in fh:
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            cases.append(json.loads(line))
    return cases


def run_case(case: dict, agent: str = "sanitize-codebase-director") -> dict:
    skill = case["skill"]
    payload = case["input"]
    start = time.time()
    cmd = [
        "apollia", "agent", "run", agent,
        "--skill", skill,
        "--json-input", json.dumps(payload),
    ]
    try:
        result = subprocess.run(
            cmd, capture_output=True, text=True, timeout=900, check=False
        )
        elapsed = time.time() - start
        try:
            out_json = json.loads(result.stdout.strip().splitlines()[-1])
        except (json.JSONDecodeError, IndexError):
            out_json = {"raw_stdout": result.stdout[-500:]}
        return {
            "case_id": case["id"],
            "exit_code": result.returncode,
            "elapsed_secs": round(elapsed, 2),
            "output": out_json,
            "stderr_tail": result.stderr[-500:],
        }
    except subprocess.TimeoutExpired:
        return {
            "case_id": case["id"],
            "exit_code": -1,
            "elapsed_secs": time.time() - start,
            "output": {},
            "stderr_tail": "timeout",
        }


def check_expectations(case: dict, run: dict) -> tuple[bool, list[str]]:
    """Apply the case's expectations dict against the run output."""
    expects = case.get("expectations", {})
    out = run.get("output", {}) or {}
    failures: list[str] = []

    if "state" in expects:
        actual = out.get("state")
        if actual != expects["state"]:
            failures.append(f"state {actual!r} != {expects['state']!r}")

    if "state_in" in expects:
        actual = out.get("state")
        if actual not in expects["state_in"]:
            failures.append(f"state {actual!r} not in {expects['state_in']}")

    if "diff_only_touches_comments" in expects:
        actual = out.get("diff_only_touches_comments")
        if bool(actual) != bool(expects["diff_only_touches_comments"]):
            failures.append(
                f"diff_only_touches_comments {actual!r} != "
                f"{expects['diff_only_touches_comments']!r}"
            )

    if "em_dash_removed_min" in expects:
        actual = int(out.get("em_dash_removed", 0))
        if actual < int(expects["em_dash_removed_min"]):
            failures.append(
                f"em_dash_removed {actual} < {expects['em_dash_removed_min']}"
            )

    if "issues_after_lte_issues_before" in expects:
        before = int(out.get("issues_before", 0))
        after = int(out.get("issues_after", 0))
        if after > before:
            failures.append(f"issues_after {after} > issues_before {before}")

    if "counts_has_done_or_residue" in expects:
        counts = out.get("counts", {}) or {}
        if counts.get("done", 0) + counts.get("residue", 0) == 0:
            failures.append("counts has no done or residue entries")

    if "metrics_tool_calls_max" in expects:
        tc = int((out.get("metrics") or {}).get("tool_calls", 0))
        if tc > int(expects["metrics_tool_calls_max"]):
            failures.append(
                f"tool_calls {tc} > max {expects['metrics_tool_calls_max']}"
            )

    return (len(failures) == 0, failures)


def main() -> int:
    parser = argparse.ArgumentParser(description="sanitize-codebase eval runner")
    parser.add_argument("--runs", type=int, default=DEFAULT_RUNS)
    parser.add_argument(
        "--success-threshold", type=float, default=DEFAULT_SUCCESS_THRESHOLD,
        help="Minimum success rate across (cases x runs)",
    )
    parser.add_argument(
        "--cases", type=str, default=None,
        help="Comma-separated case ids to run (default: all)",
    )
    args = parser.parse_args()

    cases = load_cases()
    if args.cases:
        wanted = set(args.cases.split(","))
        cases = [c for c in cases if c["id"] in wanted]
    if not cases:
        print("no cases to run", file=sys.stderr)
        return 1

    total = 0
    passed = 0
    elapsed_per_case: dict[str, list[float]] = {}

    for case in cases:
        case_id = case["id"]
        elapsed_per_case[case_id] = []
        for run_idx in range(args.runs):
            total += 1
            result = run_case(case)
            elapsed_per_case[case_id].append(result["elapsed_secs"])
            ok, failures = check_expectations(case, result)
            tag = "PASS" if ok else "FAIL"
            if ok:
                passed += 1
            print(
                f"[{tag}] {case_id} run={run_idx + 1}/{args.runs} "
                f"elapsed={result['elapsed_secs']}s "
                f"failures={failures}"
            )

    rate = passed / total if total else 0.0
    print("---")
    print(f"success rate : {passed}/{total} = {rate:.2%}")
    for cid, elapsed in elapsed_per_case.items():
        if elapsed:
            print(
                f"latency {cid}: median={statistics.median(elapsed):.2f}s "
                f"stdev={statistics.pstdev(elapsed):.2f}s"
            )

    if rate < args.success_threshold:
        print(
            f"FAIL: success rate {rate:.2%} below threshold "
            f"{args.success_threshold:.2%}",
            file=sys.stderr,
        )
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
