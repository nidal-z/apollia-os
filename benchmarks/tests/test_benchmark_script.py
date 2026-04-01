"""Tests for the Worker Agent benchmark script and supporting infrastructure."""
from __future__ import annotations

import importlib
import importlib.util
import json
import os
import sys
from pathlib import Path
from typing import Any

import pytest

_REPO_ROOT = Path(__file__).resolve().parent.parent.parent
_BENCHMARKS_DIR = _REPO_ROOT / "benchmarks"
_FIXTURES_DIR = _BENCHMARKS_DIR / "fixtures"

# Make SDK and benchmark directories importable.
if str(_REPO_ROOT / "sdk") not in sys.path:
    sys.path.insert(0, str(_REPO_ROOT / "sdk"))
if str(_BENCHMARKS_DIR) not in sys.path:
    sys.path.insert(0, str(_BENCHMARKS_DIR))


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _load_module(path: Path, module_name: str) -> Any:
    spec = importlib.util.spec_from_file_location(module_name, path)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    # Register in sys.modules before exec so dataclass string annotation
    # resolution (Python 3.10+) can look up the module's namespace.
    sys.modules[module_name] = module
    try:
        spec.loader.exec_module(module)  # type: ignore[union-attr]
    except Exception:
        sys.modules.pop(module_name, None)
        raise
    return module


# ---------------------------------------------------------------------------
# Fixture existence
# ---------------------------------------------------------------------------


def test_benchmark_fixtures_exist() -> None:
    """All three fixture files must exist before any benchmark run."""
    assert (_FIXTURES_DIR / "test_sales.xlsx").exists(), (
        "test_sales.xlsx not found — run ensure_fixtures() or "
        "python benchmarks/run_worker_benchmark.py --dry-run once"
    )
    assert (_FIXTURES_DIR / "test_corrupt.xlsx").exists(), (
        "test_corrupt.xlsx not found"
    )
    assert (_FIXTURES_DIR / "test_data_latin1.csv").exists(), (
        "test_data_latin1.csv not found"
    )


# ---------------------------------------------------------------------------
# Generic agent manifest
# ---------------------------------------------------------------------------


def test_generic_agent_manifest_valid() -> None:
    """generic-agent must expose a valid manifest with expected fields."""
    # GIVEN the generic_agent module loaded from benchmarks/
    generic = _load_module(_BENCHMARKS_DIR / "generic_agent.py", "generic_agent")
    # WHEN manifest() is called
    m = generic.agent.manifest()
    # THEN the manifest satisfies the baseline contract
    assert m["name"] == "generic-agent"
    assert m["supports_a2a"] is False
    assert m["execution_mode"] == "direct"
    assert "python_executor" in m["tools_required"]


# ---------------------------------------------------------------------------
# Benchmark script — dry-run smoke test
# ---------------------------------------------------------------------------


def test_dry_run_produces_five_results(tmp_path: Path) -> None:
    """Dry-run mode must produce exactly 5 test case entries."""
    # GIVEN the benchmark module loaded directly
    bench = _load_module(
        _BENCHMARKS_DIR / "run_worker_benchmark.py", "run_worker_benchmark"
    )
    # WHEN _build_report is called with the dry-run results
    report = bench._build_report(
        backend="llama-13b",
        runtime_url="http://localhost:7771",
        seed=42,
        n_runs=5,
        results=bench._DRY_RUN_RESULTS,
        dry_run=True,
    )
    # THEN the report contains exactly 5 test cases
    assert len(report["results"]) == 5


def test_dry_run_result_schema_valid() -> None:
    """Every dry-run result entry must contain the required metric keys."""
    bench = _load_module(
        _BENCHMARKS_DIR / "run_worker_benchmark.py", "run_worker_benchmark"
    )
    required_metrics = {"success_rate", "avg_steps", "hallucinations", "guardrails_violated"}
    for entry in bench._DRY_RUN_RESULTS:
        assert "test_case" in entry
        assert "description" in entry
        assert "worker_agent" in entry
        assert "generic_agent" in entry
        assert set(entry["worker_agent"].keys()) == required_metrics, (
            f"TC{entry['test_case']} worker_agent missing keys"
        )
        assert set(entry["generic_agent"].keys()) == required_metrics, (
            f"TC{entry['test_case']} generic_agent missing keys"
        )


def test_report_written_to_results_dir(tmp_path: Path) -> None:
    """_write_report() must create a valid JSON file in the results dir."""
    bench = _load_module(
        _BENCHMARKS_DIR / "run_worker_benchmark.py", "run_worker_benchmark"
    )
    report = bench._build_report(
        backend="test-backend",
        runtime_url="http://localhost:7771",
        seed=42,
        n_runs=1,
        results=bench._DRY_RUN_RESULTS,
        dry_run=True,
    )
    # Override the results dir to a temp location for the test
    original_dir = bench._RESULTS_DIR
    bench._RESULTS_DIR = tmp_path

    try:
        output_path = bench._write_report(report, "test-backend")
    finally:
        bench._RESULTS_DIR = original_dir

    assert output_path.exists()
    written = json.loads(output_path.read_text(encoding="utf-8"))
    assert written["metadata"]["dry_run"] is True
    assert len(written["results"]) == 5


# ---------------------------------------------------------------------------
# Instrumented tool proxy
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_tool_proxy_python_executor(tmp_path: Path) -> None:
    """python_executor must run code and return stdout."""
    bench = _load_module(
        _BENCHMARKS_DIR / "run_worker_benchmark.py", "run_worker_benchmark"
    )
    # GIVEN a proxy pointing to tmp_path
    proxy = bench._InstrumentedToolProxy(tmp_path)
    # WHEN simple Python code is executed
    result = await proxy.call("python_executor", {"code": "print('hello')"})
    # THEN stdout contains the expected output
    assert result.get("exit_code") == 0
    assert "hello" in result.get("stdout", "")
    assert proxy.step_count == 1


@pytest.mark.asyncio
async def test_tool_proxy_guardrail_detection(tmp_path: Path) -> None:
    """bash_executor with .xlsx in the command must increment guardrail_violations."""
    bench = _load_module(
        _BENCHMARKS_DIR / "run_worker_benchmark.py", "run_worker_benchmark"
    )
    # GIVEN a proxy
    proxy = bench._InstrumentedToolProxy(tmp_path)
    # WHEN bash is called with a command referencing .xlsx
    await proxy.call("bash_executor", {"command": "cat file.xlsx"})
    # THEN guardrail_violations is incremented
    assert proxy.guardrail_violations == 1


@pytest.mark.asyncio
async def test_tool_proxy_hallucination_unknown_tool(tmp_path: Path) -> None:
    """Calling a non-existent tool must increment hallucinations."""
    bench = _load_module(
        _BENCHMARKS_DIR / "run_worker_benchmark.py", "run_worker_benchmark"
    )
    # GIVEN a proxy
    proxy = bench._InstrumentedToolProxy(tmp_path)
    # WHEN an unknown tool is called
    result = await proxy.call("nonexistent_tool", {})
    # THEN hallucinations is incremented and an error is returned
    assert proxy.hallucinations == 1
    assert "error" in result


@pytest.mark.asyncio
async def test_tool_proxy_file_read_missing_file(tmp_path: Path) -> None:
    """file_read on a missing path must increment hallucinations."""
    bench = _load_module(
        _BENCHMARKS_DIR / "run_worker_benchmark.py", "run_worker_benchmark"
    )
    # GIVEN a proxy
    proxy = bench._InstrumentedToolProxy(tmp_path)
    # WHEN file_read is called with a non-existent path
    result = await proxy.call("file_read", {"path": "does_not_exist.txt"})
    # THEN hallucinations is incremented and an error is returned
    assert proxy.hallucinations == 1
    assert "error" in result


# ---------------------------------------------------------------------------
# Fixture content validation
# ---------------------------------------------------------------------------


def test_test_sales_xlsx_has_ventes_sheet() -> None:
    """test_sales.xlsx must have a sheet named 'Ventes' with 11 rows (header + 10)."""
    openpyxl = pytest.importorskip("openpyxl")
    wb = openpyxl.load_workbook(str(_FIXTURES_DIR / "test_sales.xlsx"), read_only=True)
    assert "Ventes" in wb.sheetnames
    ws = wb["Ventes"]
    rows = list(ws.iter_rows(values_only=True))
    wb.close()
    assert len(rows) == 11  # 1 header + 10 data rows
    assert rows[0] == ("Produit", "Quantite", "Prix_Unitaire")


def test_test_data_latin1_csv_is_latin1_encoded() -> None:
    """test_data_latin1.csv must be latin-1 encoded with ';' separator."""
    path = _FIXTURES_DIR / "test_data_latin1.csv"
    raw = path.read_bytes()
    # latin-1 encoded accented characters are single bytes > 0x7F
    assert any(b > 0x7F for b in raw), "File appears to be pure ASCII, not latin-1"
    decoded = raw.decode("latin-1")
    assert ";" in decoded
    assert "Nom" in decoded


def test_test_corrupt_xlsx_is_not_valid_zip() -> None:
    """test_corrupt.xlsx must not be a valid ZIP/xlsx archive."""
    import zipfile

    path = _FIXTURES_DIR / "test_corrupt.xlsx"
    assert not zipfile.is_zipfile(str(path)), (
        "test_corrupt.xlsx is a valid ZIP — it is not the intended corrupt fixture"
    )


# ---------------------------------------------------------------------------
# Test case definitions
# ---------------------------------------------------------------------------


def test_five_test_cases_defined() -> None:
    """The benchmark must define exactly 5 test cases."""
    bench = _load_module(
        _BENCHMARKS_DIR / "run_worker_benchmark.py", "run_worker_benchmark"
    )
    assert len(bench._TEST_CASES) == 5
    numbers = [tc.number for tc in bench._TEST_CASES]
    assert numbers == [1, 2, 3, 4, 5]


def test_all_test_case_fixture_files_exist() -> None:
    """Every fixture file referenced by a test case must exist."""
    bench = _load_module(
        _BENCHMARKS_DIR / "run_worker_benchmark.py", "run_worker_benchmark"
    )
    for tc in bench._TEST_CASES:
        for fixture in tc.fixture_files:
            path = _FIXTURES_DIR / fixture
            assert path.exists(), (
                f"TC{tc.number} references missing fixture: {fixture}"
            )
