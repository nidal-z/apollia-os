"""Runner d'eval pour pdf-worker.

Crée des fixtures via reportlab (sample.pdf 3 pages, with-tables.pdf,
merge-a/b/c.pdf petits, source.md, sample.txt stub) puis exécute
``eval/cases.jsonl``.

Usage :
    python3 eval/run-eval.py
    python3 eval/run-eval.py --case happy-render-md-rich
    python3 eval/run-eval.py --verbose
"""

from __future__ import annotations

import argparse
import asyncio
import importlib.util
import json
import shutil
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path
from typing import Any


# ─── Case structure ────────────────────────────────────────────────────────


@dataclass
class EvalCase:
    name: str
    payload: dict[str, Any]
    expected: dict[str, Any]
    skill_id: str | None = None
    note: str = ""


def load_cases(path: Path) -> list[EvalCase]:
    cases: list[EvalCase] = []
    for line in path.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        raw = json.loads(line)
        cases.append(
            EvalCase(
                name=raw["name"],
                payload=raw["payload"],
                expected=raw["expected"],

                skill_id=raw.get("skill_id"),
                note=raw.get("note", ""),
            )
        )
    return cases


def render_placeholders(value: Any, fx_base: str) -> Any:
    if isinstance(value, str):
        return value.replace("${FX}", fx_base)
    if isinstance(value, list):
        return [render_placeholders(v, fx_base) for v in value]
    if isinstance(value, dict):
        return {k: render_placeholders(v, fx_base) for k, v in value.items()}
    return value


# ─── Fixture builder ───────────────────────────────────────────────────────


def build_fixtures(base: Path) -> None:
    """Create test fixtures :

    - ``sample.pdf`` (3 pages, with PDF metadata)
    - ``with-tables.pdf`` (1 page with a small table)
    - ``merge-a.pdf``, ``merge-b.pdf``, ``merge-c.pdf`` (1 page each)
    - ``source.md`` (Markdown source for render-from-markdown happy path)
    - ``sample.txt`` (wrong extension, used for UNSUPPORTED_FORMAT case)
    """
    base.mkdir(parents=True, exist_ok=True)

    from reportlab.pdfgen import canvas
    from reportlab.lib.pagesizes import A4
    from reportlab.lib.units import cm
    from reportlab.platypus import SimpleDocTemplate, Paragraph, Spacer, Table, TableStyle
    from reportlab.lib.styles import getSampleStyleSheet
    from reportlab.lib import colors

    # sample.pdf — 3 pages with metadata
    sample_path = base / "sample.pdf"
    c = canvas.Canvas(str(sample_path), pagesize=A4)
    c.setTitle("Sample Fixture")
    c.setAuthor("Apollia eval")
    c.setSubject("Test PDF")
    for i in range(3):
        c.setFont("Helvetica", 16)
        c.drawString(2 * cm, 27 * cm, f"Page {i + 1}")
        c.setFont("Helvetica", 11)
        c.drawString(2 * cm, 25 * cm, f"Contenu page {i + 1} : Lorem ipsum dolor sit amet.")
        c.showPage()
    c.save()

    # with-tables.pdf — 1 page with a small table
    styles = getSampleStyleSheet()
    tables_path = base / "with-tables.pdf"
    doc = SimpleDocTemplate(str(tables_path), pagesize=A4)
    elements = [
        Paragraph("Tables fixture", styles["Heading1"]),
        Spacer(1, 12),
        Table(
            [["Métrique", "Valeur", "Évolution"],
             ["MRR", "12000", "+8%"],
             ["Churn", "2.1%", "-0.5pt"],
             ["NPS", "42", "+3"]],
            style=TableStyle([
                ("GRID", (0, 0), (-1, -1), 0.5, colors.black),
                ("BACKGROUND", (0, 0), (-1, 0), colors.lightgrey),
                ("FONTNAME", (0, 0), (-1, 0), "Helvetica-Bold"),
            ]),
        ),
    ]
    doc.build(elements)

    # merge-a/b/c.pdf — 1 page each
    for name in ("merge-a", "merge-b", "merge-c"):
        p = base / f"{name}.pdf"
        c = canvas.Canvas(str(p), pagesize=A4)
        c.setTitle(f"{name} fixture")
        c.drawString(2 * cm, 25 * cm, f"This is {name}")
        c.save()

    # source.md
    (base / "source.md").write_text(
        "# Source markdown\n\nDeuxième paragraphe.\n\n- Item 1\n- Item 2\n",
        encoding="utf-8",
    )

    # sample.txt — wrong extension stub
    (base / "sample.txt").write_text("not a pdf", encoding="utf-8")


# ─── MockCtx ───────────────────────────────────────────────────────────────


class MockCtx:
    llm = None
    workspace = None

    class _Tools:
        async def call(self, name: str, args: dict[str, Any]) -> dict[str, Any]:
            return {"error": {"code": "no_tool_in_eval", "message": f"{name} not mocked"}}

    tools = _Tools()

    def log(self, level: str, msg: str) -> None:
        if level in ("warn", "error"):
            print(f"  [worker.{level}] {msg}", file=sys.stderr)


# ─── Case runner ───────────────────────────────────────────────────────────


def _build_task(payload: dict[str, Any], skill_id: str | None) -> dict[str, Any]:
    """Build a minimal AIPTask. ``skill_id`` is propagated as the runtime would."""
    task: dict[str, Any] = {
        "task_id": "eval-task",
        "context_id": "eval-context",
        "input": {"parts": [{"type": "data", "data": payload}]},
    }
    if skill_id is not None:
        task["skill_id"] = skill_id
    return task


def _parse_output(result: dict[str, Any]) -> dict[str, Any] | None:
    try:
        return json.loads(result["output"][0]["text"])
    except (KeyError, IndexError, json.JSONDecodeError):
        return None


async def run_case(agent: Any, case: EvalCase, fx_base: str, verbose: bool) -> bool:
    payload = render_placeholders(case.payload, fx_base)
    task = _build_task(payload, case.skill_id)
    result = await agent.run(task, MockCtx())

    actual_status = result.get("status")
    expected_status = case.expected.get("status")
    if actual_status != expected_status:
        print(f"  ❌ status mismatch — expected {expected_status}, got {actual_status}")
        if verbose:
            print(f"     full result: {result}")
        return False

    if expected_status == "failed":
        expected_code = case.expected.get("code")
        actual_code = result.get("error", {}).get("code")
        if expected_code and actual_code != expected_code:
            print(f"  ❌ error code mismatch — expected {expected_code}, got {actual_code}")
            if verbose:
                print(f"     full result: {result}")
            return False

    if expected_status == "completed":
        output = _parse_output(result)
        if output is None:
            print(f"  ❌ output not parseable JSON: {result}")
            return False

        keys_expected = case.expected.get("schema_match")
        if keys_expected:
            missing = [k for k in keys_expected if k not in output]
            if missing:
                print(f"  ❌ missing keys in output : {missing}")
                if verbose:
                    print(f"     output: {output}")
                return False

    print("  ✅")
    return True


# ─── Main ──────────────────────────────────────────────────────────────────


def _load_worker_module() -> Any:
    parent = Path(__file__).resolve().parent.parent
    worker_path = parent / "pdf-worker.py"
    spec = importlib.util.spec_from_file_location("pdf_worker_under_test", str(worker_path))
    if spec is None or spec.loader is None:
        raise RuntimeError(f"unable to import worker from {worker_path}")
    module = importlib.util.module_from_spec(spec)
    sys.modules["pdf_worker_under_test"] = module
    spec.loader.exec_module(module)
    return module


async def main() -> int:
    parser = argparse.ArgumentParser(description="Eval runner for pdf-worker")
    parser.add_argument("--case", help="Run only this named case")
    parser.add_argument("--verbose", action="store_true", help="Print full results on fail")
    parser.add_argument(
        "--cases-file",
        default=str(Path(__file__).parent / "cases.jsonl"),
        help="Path to JSONL cases file",
    )
    parser.add_argument("--keep-fixtures", action="store_true", help="Keep fixture dir after run")
    args = parser.parse_args()

    worker_module = _load_worker_module()
    agent = worker_module.agent

    fx_dir = Path(tempfile.mkdtemp(prefix="pdf-worker-eval-"))
    print(f"Building fixtures in {fx_dir}")
    build_fixtures(fx_dir)
    fx_base = str(fx_dir)

    try:
        cases = load_cases(Path(args.cases_file))
        if args.case:
            cases = [c for c in cases if c.name == args.case]
            if not cases:
                print(f"No case named {args.case!r}")
                return 1

        print(f"\nRunning {len(cases)} case(s) against pdf-worker…")
        pass_count = 0
        for case in cases:
            print(f"\n• {case.name}")
            if case.note:
                print(f"  {case.note}")
            ok = await run_case(agent, case, fx_base, args.verbose)
            if ok:
                pass_count += 1

        total = len(cases)
        ratio = 100 * pass_count // total if total else 0
        print(f"\n{'=' * 60}\n{pass_count}/{total} cases passed ({ratio}%)")
        return 0 if pass_count == total else 1
    finally:
        if not args.keep_fixtures:
            shutil.rmtree(fx_dir, ignore_errors=True)
        else:
            print(f"\nFixtures kept at : {fx_dir}")


if __name__ == "__main__":
    sys.exit(asyncio.run(main()))
