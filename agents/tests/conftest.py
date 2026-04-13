"""Shared test infrastructure for Worker Agent tests.

Provides lightweight mock objects (MockTools, MockLlm, MockMemory, MockCtx)
and pytest fixtures for generating test files (Excel, CSV).

Usage::

    from conftest import MockCtx, MockTools, MockLlm, MockMemory

    tools = MockTools()
    tools.set_result("python_executor", {"stdout": "ok", "stderr": "", "exit_code": 0})
    ctx = MockCtx(tools=tools, llm=MockLlm(['{"action": "final_answer", "text": "done"}']))
    result = await agent.run({"input": {"text": "task"}}, ctx)
    assert tools.called_with("python_executor")
"""

from __future__ import annotations

import sys
from pathlib import Path
from typing import Any
from unittest.mock import MagicMock

import pytest

# Make SDK and agent directories importable regardless of pytest invocation CWD.
_REPO_ROOT = Path(__file__).resolve().parent.parent.parent
if str(_REPO_ROOT / "sdk") not in sys.path:
    sys.path.insert(0, str(_REPO_ROOT / "sdk"))
if str(_REPO_ROOT / "agents") not in sys.path:
    sys.path.insert(0, str(_REPO_ROOT / "agents"))
if str(_REPO_ROOT / "agents" / "workers") not in sys.path:
    sys.path.insert(0, str(_REPO_ROOT / "agents" / "workers"))


class MockTools:
    """Tool proxy mock that records every call and returns pre-configured results.

    Results are configured via ``set_result()`` before the test runs.
    ``called_with()`` provides a simple boolean check for post-test assertions.
    """

    def __init__(self) -> None:
        self._results: dict[str, dict[str, Any]] = {}
        self._calls: list[tuple[str, dict[str, Any]]] = []

    def set_result(self, tool_name: str, result: dict[str, Any]) -> None:
        """Register the dict that ``call()`` will return for *tool_name*."""
        self._results[tool_name] = result

    async def call(
        self,
        tool_name: str,
        input: dict[str, Any] | None = None,
    ) -> dict[str, Any]:
        """Record the invocation and return the pre-configured result.

        Raises:
            RuntimeError: If no result was configured for *tool_name*.
        """
        self._calls.append((tool_name, input or {}))
        if tool_name not in self._results:
            raise RuntimeError(
                f"MockTools: tool '{tool_name}' not configured. "
                f"Call set_result('{tool_name}', ...) before running the test."
            )
        return self._results[tool_name]

    def list_tools(self) -> list[str]:
        """Return the names of all configured tools."""
        return list(self._results.keys())

    def called_with(self, tool_name: str) -> bool:
        """Return True if *tool_name* was called at least once."""
        return any(name == tool_name for name, _ in self._calls)

    @property
    def calls(self) -> list[tuple[str, dict[str, Any]]]:
        """Ordered list of ``(tool_name, args)`` pairs recorded so far."""
        return self._calls


class MockLlm:
    """LLM mock that returns pre-configured JSON strings in FIFO order.

    Each ``complete()`` call pops the next string from the queue and attaches
    it as ``.content`` on a ``MagicMock``, matching the attribute access pattern
    of the real ``LlmProxy``.  When the queue is exhausted, ``.content`` falls
    back to ``"Tâche terminée."``.

    The strings must be valid ReAct JSON actions, e.g.::

        MockLlm([
            '{"thought": "...", "action": "tool_call", "tool": "python_executor", "args": {...}}',
            '{"thought": "done", "action": "final_answer", "text": "Résultat : 42"}',
        ])
    """

    def __init__(self, responses: list[str] | None = None) -> None:
        self._responses: list[str] = list(responses or [])
        self._index: int = 0
        self.call_count: int = 0

    async def complete(
        self,
        messages: list[dict[str, Any]],
        **kwargs: Any,
    ) -> MagicMock:
        """Return the next queued response wrapped in a ``MagicMock``."""
        self.call_count += 1
        response = MagicMock()
        if self._index < len(self._responses):
            response.content = self._responses[self._index]
            self._index += 1
        else:
            response.content = "Tâche terminée."
        return response

    @property
    def default_backend(self) -> str:
        """Return a fixed mock backend identifier."""
        return "mock"


class MockMemory:
    """Memory mock for agent unit tests.

    Supports ``remember`` / ``recall`` / ``search`` / ``record`` so the
    full ``RuntimeContext`` interface is satisfied even when memory is not
    the focus of the test.
    """

    def __init__(self) -> None:
        self._store: dict[str, str] = {}

    async def remember(
        self,
        key: str,
        value: str,
        source: str | None = None,
        confidence: float | None = None,
    ) -> None:
        """Store *value* under *key*."""
        self._store[key] = value

    async def recall(self, key: str) -> str | None:
        """Return the value stored under *key*, or ``None``."""
        return self._store.get(key)

    async def search(
        self,
        query: str,
        limit: int = 10,
    ) -> list[dict[str, Any]]:
        """Return entries whose key contains *query* (case-insensitive)."""
        return [
            {"content": v, "score": 1.0}
            for k, v in self._store.items()
            if query.lower() in k.lower()
        ][:limit]

    async def record(
        self,
        content: str,
        importance: float | None = None,
        task_id: str | None = None,
        metadata: dict[str, Any] | None = None,
    ) -> None:
        """No-op episodic recording (not used in standard unit tests)."""


class MockCtx:
    """Minimal execution context assembling tools, LLM, and memory mocks.

    Instantiate directly with keyword arguments::

        ctx = MockCtx(
            tools=MockTools(),
            llm=MockLlm(['{"action": "final_answer", "text": "ok"}']),
            memory=MockMemory(),
        )

    All arguments are optional — empty mocks are created for any omitted
    component so the agent's ``ctx.tools``, ``ctx.llm``, and ``ctx.memory``
    attributes are always non-None.
    """

    def __init__(
        self,
        tools: MockTools | None = None,
        llm: MockLlm | None = None,
        memory: MockMemory | None = None,
    ) -> None:
        self.tools: MockTools = tools if tools is not None else MockTools()
        self.llm: MockLlm = llm if llm is not None else MockLlm()
        self.memory: MockMemory = memory if memory is not None else MockMemory()


# ---------------------------------------------------------------------------
# Pytest fixtures
# ---------------------------------------------------------------------------


@pytest.fixture
def mock_ctx() -> MockCtx:
    """Return a ``MockCtx`` with empty ``MockTools`` and ``MockLlm``."""
    return MockCtx(tools=MockTools(), llm=MockLlm([]))


@pytest.fixture
def excel_file(tmp_path: Path) -> Path:
    """Create a test ``.xlsx`` file in *tmp_path* and return its ``Path``.

    The workbook has a single sheet named "Ventes" with 3 columns
    (Produit, Quantité, Prix) and 10 data rows.  Requires openpyxl.
    """
    openpyxl = pytest.importorskip("openpyxl")
    wb = openpyxl.Workbook()
    ws = wb.active
    ws.title = "Ventes"
    ws.append(["Produit", "Quantité", "Prix"])
    for i in range(1, 11):
        ws.append([f"Produit {i}", i * 2, round(i * 9.99, 2)])
    path = tmp_path / "test_sales.xlsx"
    wb.save(str(path))
    return path


@pytest.fixture
def csv_utf8_file(tmp_path: Path) -> Path:
    """Create a UTF-8 CSV file with comma separator and return its ``Path``."""
    path = tmp_path / "test_data.csv"
    path.write_text(
        "Nom,Ville,Chiffre d'affaires,Employés\n"
        "Alpha,Paris,150000,12\n"
        "Beta,Lyon,89000,5\n"
        "Gamma,Bordeaux,210000,20\n",
        encoding="utf-8",
    )
    return path


@pytest.fixture
def csv_latin1_file(tmp_path: Path) -> Path:
    """Create a latin-1 encoded CSV file and return its ``Path``.

    Simulates CSVs exported from Excel on French Windows machines where
    accented characters are encoded in latin-1 rather than UTF-8.
    """
    path = tmp_path / "test_latin1.csv"
    path.write_bytes(
        "Nom,Région,Valeur\nSociété Dupont,Île-de-France,42000\n".encode("latin-1")
    )
    return path


@pytest.fixture
def pdf_file(tmp_path: Path) -> Path:
    """Create a minimal valid PDF file in *tmp_path* and return its ``Path``.

    The file contains one page of text.  Requires pdfplumber to be importable
    (the package is declared in the pdf-worker manifest and auto-installed).
    If pdfplumber is not available, the fixture skips the test.

    The PDF is constructed as a raw byte string to avoid depending on any
    specific PDF generation library beyond what pdfplumber itself requires.
    """
    pytest.importorskip("pdfplumber")

    # Minimal hand-crafted single-page PDF (PDF 1.4 compatible).
    # The stream contains a simple text object rendering "Test PDF content".
    pdf_bytes = (
        b"%PDF-1.4\n"
        b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n"
        b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n"
        b"3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792]"
        b" /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>\nendobj\n"
        b"4 0 obj\n<< /Length 44 >>\nstream\n"
        b"BT /F1 12 Tf 72 720 Td (Test PDF content) Tj ET\n"
        b"endstream\nendobj\n"
        b"5 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>\nendobj\n"
        b"xref\n0 6\n"
        b"0000000000 65535 f \n"
        b"0000000009 00000 n \n"
        b"0000000058 00000 n \n"
        b"0000000115 00000 n \n"
        b"0000000266 00000 n \n"
        b"0000000360 00000 n \n"
        b"trailer\n<< /Size 6 /Root 1 0 R >>\n"
        b"startxref\n441\n%%EOF\n"
    )
    path = tmp_path / "test_document.pdf"
    path.write_bytes(pdf_bytes)
    return path
