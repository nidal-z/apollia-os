#!/usr/bin/env python3
"""Benchmark: excel-worker (Worker Agent) vs generic-agent on a local LLM.

Runs 5 Excel/CSV test cases comparing the Worker Agent pattern against a
generic baseline. Calls the Apollia runtime inference API (POST /api/v1/llm/complete)
so the benchmark goes through the same LlmRouter, StepBudget, and EventBus
as production agents.

Usage:
    python benchmarks/run_worker_benchmark.py --help
    python benchmarks/run_worker_benchmark.py --backend llama-13b --runs 5
    python benchmarks/run_worker_benchmark.py --dry-run
"""
from __future__ import annotations

import argparse
import asyncio
import dataclasses
import datetime
import importlib.util
import json
import logging
import platform
import shutil
import sys
import tempfile
import urllib.error
import urllib.request
from pathlib import Path
from typing import Any

# ---------------------------------------------------------------------------
# Paths
# ---------------------------------------------------------------------------

_BENCHMARKS_DIR = Path(__file__).resolve().parent
_REPO_ROOT = _BENCHMARKS_DIR.parent
_FIXTURES_DIR = _BENCHMARKS_DIR / "fixtures"
_RESULTS_DIR = _BENCHMARKS_DIR / "results"

# Make the SDK importable regardless of CWD.
if str(_REPO_ROOT / "sdk") not in sys.path:
    sys.path.insert(0, str(_REPO_ROOT / "sdk"))
if str(_REPO_ROOT / "agents") not in sys.path:
    sys.path.insert(0, str(_REPO_ROOT / "agents"))

log = logging.getLogger(__name__)

# ---------------------------------------------------------------------------
# LLM proxy
# ---------------------------------------------------------------------------


class _LlmResponse:
    """Wraps an LLM API response with a .content attribute."""

    def __init__(self, content: str) -> None:
        self.content = content


class _ApollieLlmProxy:
    """LLM proxy that calls the Apollia runtime multi-turn completion endpoint.

    Routes through ``POST /api/v1/llm/complete`` on the Apollia runtime so the
    benchmark uses the same LlmRouter, StepBudget, and EventBus as production
    agents — not a separate inference process.
    """

    def __init__(self, backend: str | None, runtime_url: str) -> None:
        self._backend = backend
        self._url = runtime_url.rstrip("/") + "/api/v1/llm/complete"

    @property
    def default_backend(self) -> str:
        """Return the configured backend name or 'apollia-default'."""
        return self._backend or "apollia-default"

    async def complete(self, messages: list[dict[str, Any]]) -> _LlmResponse:
        """Send a multi-turn completion request to the Apollia runtime."""
        payload = json.dumps({
            "messages": [
                {
                    "role": m.get("role", "user"),
                    "content": m.get("content", ""),
                }
                for m in messages
            ],
            "backend": self._backend,
        }).encode()
        req = urllib.request.Request(
            self._url,
            data=payload,
            method="POST",
            headers={"Content-Type": "application/json"},
        )
        loop = asyncio.get_running_loop()

        def _call() -> dict[str, Any]:
            try:
                with urllib.request.urlopen(req, timeout=180) as resp:
                    return json.loads(resp.read())  # type: ignore[no-any-return]
            except urllib.error.URLError as exc:
                raise RuntimeError(
                    f"Cannot reach Apollia runtime at {self._url}. "
                    "Start the runtime with `apollia-os start`, or use --dry-run. "
                    f"Error: {exc}"
                ) from exc

        response: dict[str, Any] = await loop.run_in_executor(None, _call)
        content: str = response.get("content", "")
        return _LlmResponse(content)


# ---------------------------------------------------------------------------
# Instrumented tool proxy
# ---------------------------------------------------------------------------


class _InstrumentedToolProxy:
    """Tool proxy that executes tools locally and tracks benchmark metrics.

    Tracks step count, guardrail violations (bash used for xlsx), and
    hallucinations (unknown tools or reads of non-existent file paths).
    """

    _AVAILABLE_TOOLS = [
        "python_executor",
        "file_read",
        "file_write",
        "bash_executor",
        "file_list",
    ]

    def __init__(self, workdir: Path) -> None:
        self._workdir = workdir
        self.step_count: int = 0
        self.guardrail_violations: int = 0
        self.hallucinations: int = 0

    def list_tools(self) -> list[str]:
        """Return the list of available tool names."""
        return list(self._AVAILABLE_TOOLS)

    async def call(self, tool_name: str, args: dict[str, Any]) -> dict[str, Any]:
        """Dispatch a tool call, track metrics, and return the result."""
        self.step_count += 1

        if tool_name == "python_executor":
            return await self._python_executor(args)
        if tool_name == "file_read":
            return self._file_read(args)
        if tool_name == "file_write":
            return self._file_write(args)
        if tool_name == "bash_executor":
            return await self._bash_executor(args)
        if tool_name == "file_list":
            return self._file_list(args)

        # Unknown tool name counts as a hallucination.
        self.hallucinations += 1
        return {"error": f"Tool '{tool_name}' is not available."}

    def _resolve(self, path_str: str) -> Path:
        """Resolve a path relative to the benchmark workdir."""
        p = Path(path_str)
        return p if p.is_absolute() else self._workdir / p

    async def _python_executor(self, args: dict[str, Any]) -> dict[str, Any]:
        code = str(args.get("code", ""))
        timeout = float(args.get("timeout_secs", 30))
        # Change into workdir so relative file paths work as expected.
        preamble = f"import os\nos.chdir({str(self._workdir)!r})\n"
        proc = await asyncio.create_subprocess_exec(
            sys.executable,
            "-c",
            preamble + code,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
        )
        try:
            stdout_b, stderr_b = await asyncio.wait_for(
                proc.communicate(), timeout=timeout
            )
        except asyncio.TimeoutError:
            proc.kill()
            await proc.communicate()
            return {"stdout": "", "stderr": "TimeoutError", "exit_code": 124}
        return {
            "stdout": stdout_b.decode(errors="replace"),
            "stderr": stderr_b.decode(errors="replace"),
            "exit_code": proc.returncode if proc.returncode is not None else 1,
        }

    def _file_read(self, args: dict[str, Any]) -> dict[str, Any]:
        path = self._resolve(str(args.get("path", "")))
        if not path.exists():
            self.hallucinations += 1
            return {"error": f"File not found: {path.name}"}
        try:
            return {"content": path.read_text(encoding="utf-8", errors="replace")}
        except OSError as exc:
            return {"error": str(exc)}

    def _file_write(self, args: dict[str, Any]) -> dict[str, Any]:
        path = self._resolve(str(args.get("path", "")))
        content = str(args.get("content", ""))
        try:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(content, encoding="utf-8")
            return {"written": True}
        except OSError as exc:
            return {"error": str(exc)}

    async def _bash_executor(self, args: dict[str, Any]) -> dict[str, Any]:
        cmd = str(args.get("command", args.get("cmd", "")))
        # Guardrail: using bash to manipulate xlsx/xlsm files is a violation.
        if ".xlsx" in cmd or ".xlsm" in cmd:
            self.guardrail_violations += 1
        timeout = float(args.get("timeout_secs", 30))
        proc = await asyncio.create_subprocess_shell(
            cmd,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
            cwd=str(self._workdir),
        )
        try:
            stdout_b, stderr_b = await asyncio.wait_for(
                proc.communicate(), timeout=timeout
            )
        except asyncio.TimeoutError:
            proc.kill()
            await proc.communicate()
            return {"stdout": "", "stderr": "TimeoutError", "exit_code": 124}
        return {
            "stdout": stdout_b.decode(errors="replace"),
            "stderr": stderr_b.decode(errors="replace"),
            "exit_code": proc.returncode if proc.returncode is not None else 1,
        }

    def _file_list(self, args: dict[str, Any]) -> dict[str, Any]:
        path = self._resolve(str(args.get("path", ".")))
        recursive = bool(args.get("recursive", False))
        try:
            if recursive:
                entries = [
                    str(p.relative_to(path))
                    for p in path.rglob("*")
                    if p.is_file()
                ]
            else:
                entries = [p.name for p in path.iterdir() if p.is_file()]
            return {"entries": sorted(entries)}
        except OSError as exc:
            return {"error": str(exc)}


# ---------------------------------------------------------------------------
# Benchmark execution context
# ---------------------------------------------------------------------------


@dataclasses.dataclass
class _BenchmarkContext:
    """Execution context injected into agents during benchmark runs."""

    llm: Any
    tools: _InstrumentedToolProxy
    memory: None = None


# ---------------------------------------------------------------------------
# Test cases
# ---------------------------------------------------------------------------


@dataclasses.dataclass(frozen=True)
class _TestCase:
    """Specification of a single benchmark test case."""

    number: int
    description: str
    task_text: str
    fixture_files: tuple[str, ...]
    success_keywords: tuple[str, ...]


_TEST_CASES: list[_TestCase] = [
    _TestCase(
        number=1,
        description="Lecture simple (noms de feuilles)",
        task_text=(
            "Liste tous les noms des feuilles de calcul dans le fichier "
            "test_sales.xlsx."
        ),
        fixture_files=("test_sales.xlsx",),
        success_keywords=("Ventes",),
    ),
    _TestCase(
        number=2,
        description="Totaux par colonne",
        task_text=(
            "Dans test_sales.xlsx, calcule la somme totale de la colonne "
            "Quantite et la somme totale de la colonne Prix_Unitaire. "
            "Affiche les deux résultats."
        ),
        fixture_files=("test_sales.xlsx",),
        # Sum of Quantite: 2+4+6+...+20 = 110
        # Sum of Prix_Unitaire: 9.99+19.98+...+99.90 = 549.45
        success_keywords=("110", "549", "total", "Total", "somme", "Somme"),
    ),
    _TestCase(
        number=3,
        description="CSV latin-1 + séparateur \";\"",
        task_text=(
            "Lis le fichier test_data_latin1.csv (encodage latin-1, "
            "séparateur ';') et liste les noms des colonnes."
        ),
        fixture_files=("test_data_latin1.csv",),
        success_keywords=("Nom", "Ville", "Score", "colonne", "Colonne", "header"),
    ),
    _TestCase(
        number=4,
        description="Fichier corrompu",
        task_text=(
            "Essaie de lire le fichier test_corrupt.xlsx et rapporte "
            "ce que tu trouves."
        ),
        fixture_files=("test_corrupt.xlsx",),
        success_keywords=(
            "corrompu",
            "corrupt",
            "BadZipFile",
            "invalide",
            "invalid",
            "erreur",
            "error",
            "Error",
        ),
    ),
    _TestCase(
        number=5,
        description="Modification + sauvegarde",
        task_text=(
            "Dans test_sales.xlsx, ajoute une nouvelle ligne avec "
            "Produit='Benchmark', Quantite=99, Prix_Unitaire=1.0 "
            "dans la feuille Ventes, puis sauvegarde le fichier."
        ),
        fixture_files=("test_sales.xlsx",),
        success_keywords=(
            "sauvegard",
            "saved",
            "save",
            "enregistr",
            "modifi",
            "ajout",
        ),
    ),
]

# ---------------------------------------------------------------------------
# Dry-run representative results
# ---------------------------------------------------------------------------

_DRY_RUN_RESULTS: list[dict[str, Any]] = [
    {
        "test_case": 1,
        "description": "Lecture simple (noms de feuilles)",
        "worker_agent": {
            "success_rate": 1.0,
            "avg_steps": 2.0,
            "hallucinations": 0,
            "guardrails_violated": 0,
        },
        "generic_agent": {
            "success_rate": 0.6,
            "avg_steps": 4.2,
            "hallucinations": 1,
            "guardrails_violated": 0,
        },
    },
    {
        "test_case": 2,
        "description": "Totaux par colonne",
        "worker_agent": {
            "success_rate": 1.0,
            "avg_steps": 3.0,
            "hallucinations": 0,
            "guardrails_violated": 0,
        },
        "generic_agent": {
            "success_rate": 0.4,
            "avg_steps": 5.8,
            "hallucinations": 2,
            "guardrails_violated": 0,
        },
    },
    {
        "test_case": 3,
        "description": "CSV latin-1 + séparateur \";\"",
        "worker_agent": {
            "success_rate": 1.0,
            "avg_steps": 2.0,
            "hallucinations": 0,
            "guardrails_violated": 0,
        },
        "generic_agent": {
            "success_rate": 0.2,
            "avg_steps": 6.4,
            "hallucinations": 3,
            "guardrails_violated": 0,
        },
    },
    {
        "test_case": 4,
        "description": "Fichier corrompu",
        "worker_agent": {
            "success_rate": 1.0,
            "avg_steps": 2.0,
            "hallucinations": 0,
            "guardrails_violated": 0,
        },
        "generic_agent": {
            "success_rate": 0.6,
            "avg_steps": 3.6,
            "hallucinations": 1,
            "guardrails_violated": 1,
        },
    },
    {
        "test_case": 5,
        "description": "Modification + sauvegarde",
        "worker_agent": {
            "success_rate": 1.0,
            "avg_steps": 3.0,
            "hallucinations": 0,
            "guardrails_violated": 0,
        },
        "generic_agent": {
            "success_rate": 0.4,
            "avg_steps": 5.2,
            "hallucinations": 2,
            "guardrails_violated": 2,
        },
    },
]

# ---------------------------------------------------------------------------
# Fixture generation
# ---------------------------------------------------------------------------


def ensure_fixtures() -> None:
    """Generate benchmark fixture files into benchmarks/fixtures/ if absent."""
    _FIXTURES_DIR.mkdir(parents=True, exist_ok=True)
    _ensure_test_sales_xlsx()
    _ensure_test_corrupt_xlsx()
    _ensure_test_data_latin1_csv()
    log.info("fixtures ready in %s", _FIXTURES_DIR)


def _ensure_test_sales_xlsx() -> None:
    path = _FIXTURES_DIR / "test_sales.xlsx"
    if path.exists():
        return
    try:
        import openpyxl  # type: ignore[import-untyped]
    except ImportError as exc:
        raise RuntimeError(
            "openpyxl is required to generate fixtures. "
            "Install it with: pip install openpyxl"
        ) from exc
    wb = openpyxl.Workbook()
    ws = wb.active
    ws.title = "Ventes"
    ws.append(["Produit", "Quantite", "Prix_Unitaire"])
    for i in range(1, 11):
        ws.append([f"Produit_{i:02d}", i * 2, round(i * 9.99, 2)])
    wb.save(str(path))
    log.info("generated %s", path.name)


def _ensure_test_corrupt_xlsx() -> None:
    path = _FIXTURES_DIR / "test_corrupt.xlsx"
    if path.exists():
        return
    # Write deliberately invalid bytes — not a valid ZIP/xlsx archive.
    path.write_bytes(b"This is not a valid xlsx file.\x00\xFF\xFE corrupt data.")
    log.info("generated %s", path.name)


def _ensure_test_data_latin1_csv() -> None:
    path = _FIXTURES_DIR / "test_data_latin1.csv"
    if path.exists():
        return
    content = (
        "Nom;Ville;Score\n"
        "Société Dupont;Île-de-France;42\n"
        "Café Martin;Côte d'Azur;87\n"
        "Réseau Leblanc;Rhône-Alpes;63\n"
    )
    path.write_bytes(content.encode("latin-1"))
    log.info("generated %s", path.name)


# ---------------------------------------------------------------------------
# Agent loading
# ---------------------------------------------------------------------------


def _load_excel_worker() -> Any:
    """Load the excel-worker agent from agents/excel-worker.py."""
    agent_path = _REPO_ROOT / "agents" / "excel-worker.py"
    spec = importlib.util.spec_from_file_location("excel_worker", agent_path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"Cannot load excel-worker from {agent_path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)  # type: ignore[union-attr]
    return module.agent  # type: ignore[attr-defined]


def _load_generic_agent() -> Any:
    """Load the generic-agent from benchmarks/generic_agent.py."""
    agent_path = _BENCHMARKS_DIR / "generic_agent.py"
    spec = importlib.util.spec_from_file_location("generic_agent_module", agent_path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"Cannot load generic_agent from {agent_path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)  # type: ignore[union-attr]
    return module.agent  # type: ignore[attr-defined]


# ---------------------------------------------------------------------------
# Run helpers
# ---------------------------------------------------------------------------


@dataclasses.dataclass
class _RunMetrics:
    """Metrics from a single agent execution on one test case."""

    success: bool
    steps: int
    guardrail_violations: int
    hallucinations: int
    output_text: str
    error: str | None = None


def _extract_output(result: Any) -> str:
    """Extract a plain text string from an agent run() result dict."""
    if isinstance(result, str):
        return result
    if not isinstance(result, dict):
        return str(result)
    status = result.get("status", "")
    if status == "completed":
        parts: list[str] = []
        for part in result.get("output", []):
            if isinstance(part, dict):
                parts.append(part.get("text", ""))
        return " ".join(parts)
    if status == "failed":
        error = result.get("error", {})
        if isinstance(error, dict):
            return error.get("message", str(error))
        return str(error)
    return str(result)


def _evaluate_success(output_text: str, test_case: _TestCase) -> bool:
    """Return True if the output contains at least one success keyword."""
    lower = output_text.lower()
    return any(kw.lower() in lower for kw in test_case.success_keywords)


async def _run_single(
    agent: Any,
    test_case: _TestCase,
    llm_proxy: Any,
    workdir: Path,
) -> _RunMetrics:
    """Execute one agent on one test case and return collected metrics."""
    for fixture_name in test_case.fixture_files:
        src = _FIXTURES_DIR / fixture_name
        if src.exists():
            shutil.copy2(src, workdir / fixture_name)

    tools = _InstrumentedToolProxy(workdir)
    ctx = _BenchmarkContext(llm=llm_proxy, tools=tools)
    task: dict[str, Any] = {
        "input": {"text": test_case.task_text},
        "task_id": f"bench-tc{test_case.number}",
    }

    try:
        result = await agent.run(task, ctx)
    except Exception as exc:
        return _RunMetrics(
            success=False,
            steps=tools.step_count,
            guardrail_violations=tools.guardrail_violations,
            hallucinations=tools.hallucinations,
            output_text="",
            error=str(exc),
        )

    output_text = _extract_output(result)
    success = _evaluate_success(output_text, test_case)

    return _RunMetrics(
        success=success,
        steps=tools.step_count,
        guardrail_violations=tools.guardrail_violations,
        hallucinations=tools.hallucinations,
        output_text=output_text,
    )


def _aggregate(results: list[_RunMetrics]) -> dict[str, Any]:
    """Aggregate N run metrics into a single summary dict."""
    n = len(results)
    if n == 0:
        return {
            "success_rate": 0.0,
            "avg_steps": 0.0,
            "hallucinations": 0,
            "guardrails_violated": 0,
        }
    return {
        "success_rate": round(sum(r.success for r in results) / n, 2),
        "avg_steps": round(sum(r.steps for r in results) / n, 1),
        "hallucinations": sum(r.hallucinations for r in results),
        "guardrails_violated": sum(r.guardrail_violations for r in results),
    }


async def _run_test_case_live(
    worker_agent: Any,
    generic_agent_instance: Any,
    test_case: _TestCase,
    llm_proxy: Any,
    n_runs: int,
) -> dict[str, Any]:
    """Run both agents on a test case n_runs times and aggregate metrics."""
    worker_results: list[_RunMetrics] = []
    generic_results: list[_RunMetrics] = []

    for run_idx in range(n_runs):
        log.info(
            "TC%d run %d/%d — worker_agent",
            test_case.number,
            run_idx + 1,
            n_runs,
        )
        with tempfile.TemporaryDirectory() as tmpdir:
            wr = await _run_single(worker_agent, test_case, llm_proxy, Path(tmpdir))
            worker_results.append(wr)

        log.info(
            "TC%d run %d/%d — generic_agent",
            test_case.number,
            run_idx + 1,
            n_runs,
        )
        with tempfile.TemporaryDirectory() as tmpdir:
            gr = await _run_single(
                generic_agent_instance, test_case, llm_proxy, Path(tmpdir)
            )
            generic_results.append(gr)

    return {
        "test_case": test_case.number,
        "description": test_case.description,
        "worker_agent": _aggregate(worker_results),
        "generic_agent": _aggregate(generic_results),
    }


# ---------------------------------------------------------------------------
# Output
# ---------------------------------------------------------------------------


def _build_report(
    backend: str,
    runtime_url: str,
    seed: int,
    n_runs: int,
    results: list[dict[str, Any]],
    dry_run: bool,
) -> dict[str, Any]:
    """Assemble the final benchmark report dict."""
    return {
        "metadata": {
            "backend": backend,
            "runtime_url": runtime_url,
            "hardware": platform.node() or platform.machine(),
            "platform": platform.platform(),
            "seed": seed,
            "runs_per_test": n_runs,
            "timestamp": datetime.datetime.now(datetime.timezone.utc).strftime(
                "%Y-%m-%dT%H:%M:%SZ"
            ),
            "dry_run": dry_run,
        },
        "results": results,
    }


def _write_report(report: dict[str, Any], backend: str) -> Path:
    """Write the report JSON to benchmarks/results/ and return the path."""
    _RESULTS_DIR.mkdir(parents=True, exist_ok=True)
    safe_backend = backend.replace(":", "-").replace("/", "-")
    timestamp = datetime.datetime.now(datetime.timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    filename = f"worker-benchmark-{safe_backend}-{timestamp}.json"
    output_path = _RESULTS_DIR / filename
    output_path.write_text(
        json.dumps(report, indent=2, ensure_ascii=False),
        encoding="utf-8",
    )
    return output_path


def _print_summary(report: dict[str, Any]) -> None:
    """Print a human-readable benchmark summary to stdout."""
    meta = report["metadata"]
    tag = " [DRY-RUN]" if meta.get("dry_run") else ""
    print(f"\n=== Worker Agent Benchmark{tag} ===")
    print(f"Backend: {meta['backend']}  ({meta.get('runtime_url', '')})")
    print(f"Seed   : {meta['seed']}  Runs/test: {meta['runs_per_test']}")
    print(f"Date   : {meta['timestamp']}\n")

    header = f"{'TC':<4} {'Description':<38} {'W:succ':>7} {'G:succ':>7} {'W:steps':>8} {'G:steps':>8}"
    print(header)
    print("-" * len(header))

    for entry in report["results"]:
        wa = entry["worker_agent"]
        ga = entry["generic_agent"]
        print(
            f"{entry['test_case']:<4} "
            f"{entry['description']:<38} "
            f"{wa['success_rate']:>7.0%} "
            f"{ga['success_rate']:>7.0%} "
            f"{wa['avg_steps']:>8.1f} "
            f"{ga['avg_steps']:>8.1f}"
        )
    print()


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------


def _build_arg_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="run_worker_benchmark",
        description=(
            "Benchmark excel-worker (Worker Agent) vs generic-agent on 5 Excel/CSV "
            "test cases. Calls the Apollia runtime inference API so the benchmark "
            "goes through the same LlmRouter as production agents."
        ),
    )
    parser.add_argument(
        "--backend",
        default=None,
        help=(
            "Apollia backend name to use (default: runtime default). "
            "Configure backends with `apollia-os llm backends`."
        ),
    )
    parser.add_argument(
        "--runtime-url",
        default="http://localhost:7771",
        help="Apollia runtime URL (default: http://localhost:7771)",
    )
    parser.add_argument(
        "--runs",
        type=int,
        default=5,
        help="Number of runs per test case (default: 5)",
    )
    parser.add_argument(
        "--seed",
        type=int,
        default=42,
        help="Seed stored in report metadata for reproducibility (default: 42)",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Skip LLM calls and return representative mock results",
    )
    parser.add_argument(
        "--output",
        help="Override the output JSON file path (default: auto-generated in results/)",
    )
    parser.add_argument(
        "--verbose",
        action="store_true",
        help="Enable verbose logging",
    )
    return parser


async def _run(args: argparse.Namespace) -> int:
    """Async entry point for the benchmark."""
    logging.basicConfig(
        level=logging.DEBUG if args.verbose else logging.INFO,
        format="%(levelname)s %(message)s",
    )

    ensure_fixtures()

    runtime_url: str = args.runtime_url
    backend: str = args.backend or "apollia-default"

    if args.dry_run:
        log.info("dry-run mode — skipping runtime call, returning representative results")
        results = _DRY_RUN_RESULTS
    else:
        worker_agent = _load_excel_worker()
        generic_agent_instance = _load_generic_agent()
        llm_proxy = _ApollieLlmProxy(
            backend=args.backend,
            runtime_url=runtime_url,
        )

        results = []
        for tc in _TEST_CASES:
            log.info("Running test case %d: %s", tc.number, tc.description)
            entry = await _run_test_case_live(
                worker_agent,
                generic_agent_instance,
                tc,
                llm_proxy,
                n_runs=args.runs,
            )
            results.append(entry)

    report = _build_report(
        backend=backend,
        runtime_url=runtime_url,
        seed=args.seed,
        n_runs=args.runs,
        results=results,
        dry_run=args.dry_run,
    )

    if args.output:
        output_path = Path(args.output)
        output_path.parent.mkdir(parents=True, exist_ok=True)
        output_path.write_text(
            json.dumps(report, indent=2, ensure_ascii=False), encoding="utf-8"
        )
    else:
        output_path = _write_report(report, backend)

    _print_summary(report)
    print(f"Report written to: {output_path}")
    return 0


def main() -> None:
    """CLI entry point."""
    parser = _build_arg_parser()
    args = parser.parse_args()
    sys.exit(asyncio.run(_run(args)))


if __name__ == "__main__":
    main()
