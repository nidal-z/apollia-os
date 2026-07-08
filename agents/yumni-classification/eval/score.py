#!/usr/bin/env python3
"""Standalone scorer for the ESRS-classification PoC.

NO Apollia runtime is required to run this file - pure stdlib. It compares a
PREDICTIONS file against the labeled dataset and prints precision / recall / F1
(micro over all (sample, code) pairs, plus per-sample averages) and a short
confidence-calibration note.

--------------------------------------------------------------------------
Generating a predictions file
--------------------------------------------------------------------------
Once the Apollia runtime + a local model are available, run the director for
each sample and collect its accepted criterion codes:

    apollia-os run rse-classification-director '{"actionId": "act-001"}'

The director returns {"result": {"actionId", "accepted": [{criterionCode,...}]}}.
Build a predictions file mirroring dataset.json but adding a "predicted" list
of codes per sample:

    {
      "samples": [
        {"id": "act-001", "predicted": ["E1-3", "E1-1"],
         "confidence": {"E1-3": 0.82, "E1-1": 0.55}},
        ...
      ]
    }

(The optional "confidence" map per sample powers the calibration note; the
values are the director's accepted-mapping confidences.)

Then:

    python eval/score.py                         # uses eval/predictions.json
    python eval/score.py --pred my_preds.json    # custom predictions file
    python eval/score.py --dataset eval/dataset.json --pred eval/predictions.json
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any

HERE = Path(__file__).resolve().parent


def _load(path: Path) -> dict[str, Any]:
    with path.open(encoding="utf-8") as fh:
        data = json.load(fh)
    if not isinstance(data, dict):
        raise SystemExit(f"{path}: expected a JSON object at the root")
    return data


def _index_by_id(samples: list[dict[str, Any]]) -> dict[str, dict[str, Any]]:
    out: dict[str, dict[str, Any]] = {}
    for i, s in enumerate(samples):
        sid = str(s.get("id") or f"idx-{i}")
        out[sid] = s
    return out


def _prf(tp: int, fp: int, fn: int) -> tuple[float, float, float]:
    precision = tp / (tp + fp) if (tp + fp) else 0.0
    recall = tp / (tp + fn) if (tp + fn) else 0.0
    f1 = (2 * precision * recall / (precision + recall)) if (precision + recall) else 0.0
    return precision, recall, f1


def score(dataset: dict[str, Any], predictions: dict[str, Any]) -> dict[str, Any]:
    gold = _index_by_id(dataset.get("samples", []))
    pred = _index_by_id(predictions.get("samples", []))

    micro_tp = micro_fp = micro_fn = 0
    per_sample: list[dict[str, Any]] = []
    # confidence calibration accumulators
    correct_conf: list[float] = []
    wrong_conf: list[float] = []

    for sid, g in gold.items():
        expected = {str(c) for c in g.get("expected", [])}
        p = pred.get(sid, {})
        predicted = {str(c) for c in p.get("predicted", [])}
        conf_map = p.get("confidence", {}) if isinstance(p.get("confidence"), dict) else {}

        tp = len(expected & predicted)
        fp = len(predicted - expected)
        fn = len(expected - predicted)
        micro_tp += tp
        micro_fp += fp
        micro_fn += fn

        for code in predicted:
            c = conf_map.get(code)
            if isinstance(c, (int, float)):
                (correct_conf if code in expected else wrong_conf).append(float(c))

        pr, rc, f1 = _prf(tp, fp, fn)
        per_sample.append(
            {
                "id": sid,
                "expected": sorted(expected),
                "predicted": sorted(predicted),
                "tp": tp,
                "fp": fp,
                "fn": fn,
                "precision": round(pr, 3),
                "recall": round(rc, 3),
                "f1": round(f1, 3),
            }
        )

    micro_p, micro_r, micro_f1 = _prf(micro_tp, micro_fp, micro_fn)
    n = len(per_sample) or 1
    macro_p = sum(s["precision"] for s in per_sample) / n
    macro_r = sum(s["recall"] for s in per_sample) / n
    macro_f1 = sum(s["f1"] for s in per_sample) / n

    avg_correct = sum(correct_conf) / len(correct_conf) if correct_conf else None
    avg_wrong = sum(wrong_conf) / len(wrong_conf) if wrong_conf else None

    return {
        "n_samples": len(per_sample),
        "micro": {
            "tp": micro_tp,
            "fp": micro_fp,
            "fn": micro_fn,
            "precision": round(micro_p, 3),
            "recall": round(micro_r, 3),
            "f1": round(micro_f1, 3),
        },
        "macro": {
            "precision": round(macro_p, 3),
            "recall": round(macro_r, 3),
            "f1": round(macro_f1, 3),
        },
        "calibration": {
            "avg_confidence_correct": (round(avg_correct, 3) if avg_correct is not None else None),
            "avg_confidence_wrong": (round(avg_wrong, 3) if avg_wrong is not None else None),
        },
        "per_sample": per_sample,
    }


def _calibration_note(cal: dict[str, Any]) -> str:
    ac = cal.get("avg_confidence_correct")
    aw = cal.get("avg_confidence_wrong")
    if ac is None and aw is None:
        return "Calibration: no per-code confidences in predictions - add a 'confidence' map to enable."
    parts = []
    if ac is not None:
        parts.append(f"avg confidence on CORRECT codes = {ac}")
    if aw is not None:
        parts.append(f"avg confidence on WRONG codes = {aw}")
    verdict = ""
    if ac is not None and aw is not None:
        gap = round(ac - aw, 3)
        if gap > 0.1:
            verdict = f" -> well separated (gap {gap}): confidence is informative."
        elif gap > 0:
            verdict = f" -> weakly separated (gap {gap}): consider raising THRESHOLD."
        else:
            verdict = f" -> NOT separated (gap {gap}): confidence is not discriminative, review prompts."
    return "Calibration: " + "; ".join(parts) + verdict


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Score ESRS-classification predictions.")
    parser.add_argument("--dataset", default=str(HERE / "dataset.json"))
    parser.add_argument("--pred", default=str(HERE / "predictions.json"))
    parser.add_argument("--json", action="store_true", help="emit full JSON report")
    args = parser.parse_args(argv)

    dataset = _load(Path(args.dataset))
    pred_path = Path(args.pred)
    if not pred_path.exists():
        print(
            f"Predictions file not found: {pred_path}\n"
            "Generate one by running the director per sample (see this file's docstring).",
            file=sys.stderr,
        )
        return 2
    predictions = _load(pred_path)

    report = score(dataset, predictions)

    if args.json:
        print(json.dumps(report, indent=2, ensure_ascii=False))
        return 0

    m = report["micro"]
    ma = report["macro"]
    print(f"Samples scored: {report['n_samples']}")
    print(
        f"Micro  P={m['precision']}  R={m['recall']}  F1={m['f1']}  "
        f"(tp={m['tp']} fp={m['fp']} fn={m['fn']})"
    )
    print(f"Macro  P={ma['precision']}  R={ma['recall']}  F1={ma['f1']}")
    print(_calibration_note(report["calibration"]))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
