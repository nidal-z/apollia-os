#!/usr/bin/env python3
"""Non-regression check against the frozen ESRS baseline.

Scores a fresh predictions file with the same scorer used to freeze the baseline
(eval/score.py) and compares the micro-F1 to the frozen reference in
baseline-scores.json. Exits non-zero if the fresh run regresses past the tolerance.

This is a baseline SCORE comparison, not a runtime-trace replay: it re-scores a new
run against a frozen expected score, it does not re-execute a recorded trace.

Usage:
    python eval/fixtures/check_baseline.py --pred eval/predictions.json
    python eval/fixtures/check_baseline.py --pred fresh.json --tolerance 0.05
"""

import argparse
import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
EVAL = HERE.parent
sys.path.insert(0, str(EVAL))

from score import score  # noqa: E402  (eval/score.py, pure stdlib)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Check a run against the frozen ESRS baseline."
    )
    parser.add_argument("--pred", required=True, help="fresh predictions file to check")
    parser.add_argument("--dataset", default=str(EVAL / "dataset.json"))
    parser.add_argument("--baseline", default=str(HERE / "baseline-scores.json"))
    parser.add_argument(
        "--tolerance",
        type=float,
        default=0.05,
        help="allowed micro-F1 drop below the baseline before flagging a regression",
    )
    args = parser.parse_args(argv)

    dataset = json.loads(Path(args.dataset).read_text(encoding="utf-8"))
    fresh = json.loads(Path(args.pred).read_text(encoding="utf-8"))
    baseline = json.loads(Path(args.baseline).read_text(encoding="utf-8"))

    report = score(dataset, fresh)
    fresh_f1 = report["micro"]["f1"]
    base_f1 = baseline["micro"]["f1"]
    floor = round(base_f1 - args.tolerance, 3)

    print(
        f"baseline micro-F1 = {base_f1}   fresh micro-F1 = {fresh_f1}   floor = {floor}"
    )
    if fresh_f1 + 1e-9 < floor:
        print(f"REGRESSION: micro-F1 {fresh_f1} below floor {floor}", file=sys.stderr)
        return 1
    print("OK: no regression against the baseline.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
