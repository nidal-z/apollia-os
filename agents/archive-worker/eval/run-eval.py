"""Runner d'eval pour archive-worker.

Crée des fixtures temporaires (zips/tars d'exemple, archive piégée pour path
traversal, sources de création), exécute les cas définis dans
``eval/cases.jsonl`` et compare les résultats. Aucun mock Apollia LLM/tools
nécessaire — le worker est déterministe.

Usage :
    python3 eval/run-eval.py
    python3 eval/run-eval.py --case happy-list-zip
    python3 eval/run-eval.py --verbose
"""

from __future__ import annotations

import argparse
import asyncio
import importlib.util
import json
import os
import shutil
import sys
import tarfile
import tempfile
import zipfile
from dataclasses import dataclass
from pathlib import Path
from typing import Any


# ─── Case structure ────────────────────────────────────────────────────────


@dataclass
class EvalCase:
    name: str
    payload: dict[str, Any]
    expected: dict[str, Any]
    # skill_id propagé dans le mock AIPTask. ``None`` simule un appel root
    # (CLI/trigger/REST) sans skill ciblé — utile pour tester l'erreur
    # ``MISSING_SKILL_ID``.
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
    """Replace ``${FX}`` placeholder recursively in payload values."""
    if isinstance(value, str):
        return value.replace("${FX}", fx_base)
    if isinstance(value, list):
        return [render_placeholders(v, fx_base) for v in value]
    if isinstance(value, dict):
        return {k: render_placeholders(v, fx_base) for k, v in value.items()}
    return value


# ─── Fixture builder ───────────────────────────────────────────────────────


def build_fixtures(base: Path) -> None:
    """Create test fixtures in ``base`` :

    - ``sample.zip``    : 3 entries (hello.txt, data/binary.bin, dir/)
    - ``sample.tar.gz`` : same logical content, tar.gz format
    - ``evil.zip``      : entry with ``../escape.txt`` to test path traversal
    - ``sample.rar``    : empty file with unsupported extension
    - ``src/a.txt`` + ``src/b.txt`` : sources for archive.create cases
    """
    base.mkdir(parents=True, exist_ok=True)

    # Sources for create cases
    src = base / "src"
    src.mkdir(exist_ok=True)
    (src / "a.txt").write_text("file A — alpha\n", encoding="utf-8")
    (src / "b.txt").write_text("file B — beta\n", encoding="utf-8")

    # sample.zip
    sample_zip = base / "sample.zip"
    with zipfile.ZipFile(sample_zip, "w", compression=zipfile.ZIP_DEFLATED) as zf:
        zf.writestr("hello.txt", "Hello, Apollia! — UTF-8 accents : éàü\n")
        zf.writestr("data/binary.bin", b"\x00\x01\x02\x03binary\xff\xfe")
        zi = zipfile.ZipInfo("dir/")
        zi.external_attr = 0o40755 << 16
        zf.writestr(zi, "")

    # sample.tar.gz
    sample_targz = base / "sample.tar.gz"
    with tarfile.open(sample_targz, "w:gz") as tf:
        for name, content in (
            ("hello.txt", "Hello, Apollia (tar) - accents : eee\n".encode("utf-8")),
            ("data/binary.bin", b"\x00\x01\x02\x03binary\xff"),
        ):
            info = tarfile.TarInfo(name=name)
            info.size = len(content)
            tf.addfile(info, fileobj=_as_fileobj(content))

    # evil.zip — path traversal entry
    evil_zip = base / "evil.zip"
    with zipfile.ZipFile(evil_zip, "w", compression=zipfile.ZIP_DEFLATED) as zf:
        zf.writestr("../escape.txt", "should be rejected")
        zf.writestr("safe.txt", "this one is fine")

    # sample.rar — unsupported format marker (empty stub)
    (base / "sample.rar").write_bytes(b"Rar!\x1a\x07\x00")


def _as_fileobj(data: bytes):
    import io

    return io.BytesIO(data)


# ─── MockCtx ───────────────────────────────────────────────────────────────


class MockCtx:
    """Minimal RuntimeContext stub — sufficient for a deterministic worker."""

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
    """Build a minimal AIPTask whose input wraps ``payload`` in a DataPart.

    ``skill_id`` is propagated as the runtime would via the A2A delegate, so
    multi-skill workers can dispatch via ``extract_skill_id(task)``. Pass
    ``None`` to simulate a root invocation (CLI/trigger/REST) without skill
    targeting.
    """
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

        # schema_match : liste des clés top-level qui doivent exister
        keys_expected = case.expected.get("schema_match")
        if keys_expected:
            missing = [k for k in keys_expected if k not in output]
            if missing:
                print(f"  ❌ missing keys in output : {missing}")
                if verbose:
                    print(f"     output: {output}")
                return False

        # skipped_reasons_at_least : compteurs minimums attendus
        sr_min = case.expected.get("skipped_reasons_at_least")
        if sr_min:
            actual_sr = output.get("skipped_reasons", {})
            for key, min_val in sr_min.items():
                got = actual_sr.get(key, 0)
                if got < min_val:
                    print(
                        f"  ❌ skipped_reasons.{key} expected ≥ {min_val}, got {got}"
                    )
                    if verbose:
                        print(f"     full output: {output}")
                    return False

    print("  ✅")
    return True


# ─── Main ──────────────────────────────────────────────────────────────────


def _load_worker_module() -> Any:
    """Import archive-worker.py without relying on the package name."""
    parent = Path(__file__).resolve().parent.parent
    worker_path = parent / "archive-worker.py"
    spec = importlib.util.spec_from_file_location("archive_worker_under_test", str(worker_path))
    if spec is None or spec.loader is None:
        raise RuntimeError(f"unable to import worker from {worker_path}")
    module = importlib.util.module_from_spec(spec)
    sys.modules["archive_worker_under_test"] = module
    spec.loader.exec_module(module)
    return module


async def main() -> int:
    parser = argparse.ArgumentParser(description="Eval runner for archive-worker")
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

    fx_dir = Path(tempfile.mkdtemp(prefix="archive-worker-eval-"))
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

        print(f"\nRunning {len(cases)} case(s) against archive-worker…")
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
