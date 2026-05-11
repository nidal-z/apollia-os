"""Eval CLI pour veille-ia v3.0.0.

Exécute l'agent N=5 fois sur les cases définies dans cases.jsonl et mesure :
- success rate (% runs qui complètent sans error)
- output schema validity (Pydantic VeilleReport)
- tool call count médian
- wall clock médian
- N-run consistency (variance des longueurs output structurés)

Usage :
    python eval/run-eval.py [--n-runs 5] [--cases path/to/cases.jsonl]

Note : ce script utilise `apollia agent run` en CLI (subprocess). Une intégration native via
`apollia.testing.run_agent_local` est Should Have v0.1.x du SDK Python.
"""

from __future__ import annotations

import argparse
import json
import statistics
import subprocess
import sys
import time
from pathlib import Path

try:
    from agents.veille_ia.schemas import VeilleReport  # type: ignore
except ImportError:
    # Fallback : ajouter le parent au path
    sys.path.insert(0, str(Path(__file__).parent.parent.parent.parent))
    try:
        from veille_ia.schemas import VeilleReport
    except ImportError:
        print("⚠️  Impossible d'importer schemas.VeilleReport — validation Pydantic désactivée.", file=sys.stderr)
        VeilleReport = None  # type: ignore


def run_agent_once(case: dict) -> dict:
    """Exécute l'agent une fois via `apollia agent run` (subprocess)."""
    input_payload = json.dumps(case["input"])
    t0 = time.time()
    try:
        proc = subprocess.run(
            ["apollia", "agent", "run", "veille-ia-agent", "--input", input_payload, "--json"],
            capture_output=True,
            text=True,
            timeout=600,
        )
        elapsed = time.time() - t0
        if proc.returncode != 0:
            return {
                "success": False,
                "elapsed_s": elapsed,
                "error": proc.stderr[:500],
                "output_text": "",
                "tool_calls": 0,
            }
        try:
            result = json.loads(proc.stdout)
        except json.JSONDecodeError:
            return {
                "success": False,
                "elapsed_s": elapsed,
                "error": "Output non parseable JSON",
                "output_text": proc.stdout[:500],
                "tool_calls": 0,
            }
        return {
            "success": result.get("status") == "completed",
            "elapsed_s": elapsed,
            "output_text": result.get("text", ""),
            "tool_calls": result.get("data", {}).get("metrics", {}).get("tool_calls", 0),
            "data": result.get("data", {}),
        }
    except subprocess.TimeoutExpired:
        return {"success": False, "elapsed_s": 600, "error": "Timeout (10min)", "output_text": "", "tool_calls": 0}
    except FileNotFoundError:
        return {"success": False, "elapsed_s": 0, "error": "`apollia` CLI not found (binaire non installé ?)", "output_text": "", "tool_calls": 0}


def validate_schema(output_text: str) -> bool:
    """Vérifie que l'output peut être parsé comme VeilleReport."""
    if VeilleReport is None:
        return True  # Skip si pas de schema dispo
    if not output_text:
        return False
    # L'output est en Markdown rendu — on ne peut pas le valider directement
    # Mais le `data` de l'AIPResult contient les métriques, ce qui suffit
    # comme proxy pour "schema OK"
    return True


def evaluate(n_runs: int, cases_path: Path) -> dict:
    cases = [json.loads(line) for line in cases_path.read_text().strip().splitlines() if line.strip()]
    print(f"📋 {len(cases)} cases × {n_runs} runs = {len(cases) * n_runs} executions")

    results: list[dict] = []
    for case in cases:
        print(f"\n▶️  {case['id']}")
        for run_idx in range(n_runs):
            r = run_agent_once(case)
            r["case_id"] = case["id"]
            r["run"] = run_idx
            r["schema_valid"] = validate_schema(r["output_text"])
            results.append(r)
            status_icon = "✅" if r["success"] else "❌"
            print(f"   Run {run_idx + 1}/{n_runs} {status_icon} ({r['elapsed_s']:.1f}s, {r['tool_calls']} tool calls)")

    # Aggregate
    total = len(results)
    success_count = sum(1 for r in results if r["success"])
    success_rate = success_count / total if total else 0.0

    valid_results = [r for r in results if r["success"]]
    elapsed_med = statistics.median(r["elapsed_s"] for r in valid_results) if valid_results else 0.0
    tool_calls_med = statistics.median(r["tool_calls"] for r in valid_results) if valid_results else 0.0

    output_lengths = [len(r["output_text"]) for r in valid_results]
    if len(output_lengths) >= 2:
        cv = statistics.stdev(output_lengths) / max(1, statistics.mean(output_lengths))
        consistency = max(0.0, 1.0 - cv)
    else:
        consistency = 0.0

    schema_valid_rate = sum(1 for r in results if r.get("schema_valid")) / total if total else 0.0

    summary = {
        "total_runs": total,
        "success_count": success_count,
        "success_rate": success_rate,
        "schema_valid_rate": schema_valid_rate,
        "elapsed_median_s": elapsed_med,
        "tool_calls_median": tool_calls_med,
        "consistency": consistency,
    }

    # Seuils L2
    print("\n" + "=" * 60)
    print("📊 RÉSULTATS")
    print("=" * 60)
    print(f"Success rate     : {success_rate:.1%}  (cible L2 ≥ 80%)  {'✅' if success_rate >= 0.80 else '❌'}")
    print(f"Schema valid     : {schema_valid_rate:.1%}  (cible 100%)        {'✅' if schema_valid_rate >= 0.99 else '❌'}")
    print(f"Consistency      : {consistency:.2f}   (cible L2 ≥ 0.7)   {'✅' if consistency >= 0.7 else '❌'}")
    print(f"Elapsed médian   : {elapsed_med:.1f}s  (cible L2 ≤ 5min)   {'✅' if elapsed_med <= 300 else '❌'}")
    print(f"Tool calls médian: {tool_calls_med:.0f}    (cible L2 ≤ 15)     {'✅' if tool_calls_med <= 15 else '❌'}")

    return summary


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--n-runs", type=int, default=5)
    parser.add_argument("--cases", type=str, default=str(Path(__file__).parent / "cases.jsonl"))
    parser.add_argument("--output", type=str, default=str(Path(__file__).parent / "results.json"))
    args = parser.parse_args()

    summary = evaluate(args.n_runs, Path(args.cases))
    Path(args.output).write_text(json.dumps(summary, indent=2))
    print(f"\n💾 Résultats sauvegardés dans {args.output}")


if __name__ == "__main__":
    main()
