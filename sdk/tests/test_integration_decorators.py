"""End-to-end integration tests for the public decorator API.

Validates that a class decorated with ``@agent`` + handlers is fully
operational through ``__apollia_dispatch__``.
"""

from __future__ import annotations

import asyncio
import sys
import types
from types import SimpleNamespace
from typing import Any

from apollia._internal.manifest import MANIFEST_ATTR
from apollia.agent import agent
from apollia.errors import DomainError, NeedHumanInput
from apollia.messages import on_message
from apollia.orchestration import orchestrated
from apollia.skills import skill

_COUNTER = 0


def _new_module() -> str:
    global _COUNTER
    _COUNTER += 1
    name = f"test_integration_decorators_mod_{_COUNTER}"
    sys.modules[name] = types.ModuleType(name)
    return name


# ──────────────────────────────────────────────────────────────────────
# Pure-skill agent
# ──────────────────────────────────────────────────────────────────────


def test_pure_skill_agent_dispatches_skill() -> None:
    mod_name = _new_module()

    class TestPdf:
        @skill("pdf.read", description="Read PDF")
        async def read(self, path: str, ctx: Any = None) -> dict[str, Any]:
            return {"path": path, "ok": True}

    TestPdf.__module__ = mod_name
    TestPdf = agent(name="test-pdf", version="0.1.0", description="pdf tool")(  # NOSONAR
        TestPdf
    )

    result = asyncio.run(
        TestPdf().__apollia_dispatch__(  # type: ignore[attr-defined]
            {
                "skill_id": "pdf.read",
                "input": {"parts": [{"type": "data", "data": {"path": "a.pdf"}}]},
            },
            SimpleNamespace(logger=None),
        )
    )
    assert result["status"] == "completed"


def test_pure_skill_agent_unknown_skill_id() -> None:
    mod_name = _new_module()

    class A:
        @skill("known.skill")
        async def k(self, ctx: Any = None) -> dict[str, int]:
            return {}

    A.__module__ = mod_name
    A = agent(name="a", version="0.1.0", description="d")(A)

    result = asyncio.run(
        A().__apollia_dispatch__(  # type: ignore[attr-defined]
            {"skill_id": "unknown.skill", "input": {"parts": []}},
            SimpleNamespace(logger=None),
        )
    )
    assert result["status"] == "failed"
    assert "UNKNOWN_SKILL_ID" in result.get("error", {}).get("code", "")


# ──────────────────────────────────────────────────────────────────────
# Pure-conversational agent
# ──────────────────────────────────────────────────────────────────────


def test_pure_conversational_agent_dispatches_message() -> None:
    mod_name = _new_module()

    class Chat:
        @on_message
        async def chat(self, message: str, history: list, ctx: Any) -> str:
            return f"echo:{message}"

    Chat.__module__ = mod_name
    Chat = agent(name="chat", version="0.1.0", description="echo")(Chat)  # NOSONAR

    result = asyncio.run(
        Chat().__apollia_dispatch__(  # type: ignore[attr-defined]
            {"input": {"parts": [{"type": "text", "text": "hi"}]}},
            SimpleNamespace(logger=None),
        )
    )
    assert result["status"] == "completed"


# ──────────────────────────────────────────────────────────────────────
# Pure-orchestrated agent
# ──────────────────────────────────────────────────────────────────────


def test_pure_orchestrated_agent_has_no_dispatch_route() -> None:
    """Orchestrated agents are driven by ORIA, not the dispatch loop -
    when called via __apollia_dispatch__ without a skill_id, they fail."""
    mod_name = _new_module()

    @orchestrated(system_prompt="You are a researcher.")
    class Researcher:
        pass

    Researcher.__module__ = mod_name
    Researcher = agent(  # NOSONAR
        name="researcher", version="0.1.0", description="research"
    )(Researcher)

    manifest = getattr(Researcher, MANIFEST_ATTR)
    assert manifest["execution_mode"] == "orchestrated"
    assert manifest["system_prompt"] == "You are a researcher."

    result = asyncio.run(
        Researcher().__apollia_dispatch__(  # type: ignore[attr-defined]
            {"input": {"parts": []}},
            SimpleNamespace(logger=None),
        )
    )
    assert result["status"] == "failed"


# ──────────────────────────────────────────────────────────────────────
# DomainError trapping
# ──────────────────────────────────────────────────────────────────────


def test_skill_handler_domain_error_trapped() -> None:
    mod_name = _new_module()

    class A:
        @skill("fail.op")
        async def fail(self, ctx: Any = None) -> dict[str, int]:
            raise DomainError("BOOM", "something broke", {"why": "test"})

    A.__module__ = mod_name
    A = agent(name="a", version="0.1.0", description="d")(A)

    result = asyncio.run(
        A().__apollia_dispatch__(  # type: ignore[attr-defined]
            {"skill_id": "fail.op", "input": {"parts": []}},
            SimpleNamespace(logger=None),
        )
    )
    assert result["status"] == "failed"
    assert result["error"]["code"] == "BOOM"


def test_skill_handler_need_human_input() -> None:
    mod_name = _new_module()

    class A:
        @skill("ask.op")
        async def ask(self, ctx: Any = None) -> dict[str, int]:
            raise NeedHumanInput("confirm?", {"step": 1})

    A.__module__ = mod_name
    A = agent(name="a", version="0.1.0", description="d")(A)

    result = asyncio.run(
        A().__apollia_dispatch__(  # type: ignore[attr-defined]
            {"skill_id": "ask.op", "input": {"parts": []}},
            SimpleNamespace(logger=None),
        )
    )
    assert result["status"] == "input_required"


# ──────────────────────────────────────────────────────────────────────
# Multi-skill agent + payload validation
# ──────────────────────────────────────────────────────────────────────


def test_multi_skill_agent_routes_correctly() -> None:
    mod_name = _new_module()

    class Calc:
        @skill("calc.add")
        async def add(self, a: int, b: int, ctx: Any = None) -> dict[str, int]:
            return {"r": a + b}

        @skill("calc.mul")
        async def mul(self, a: int, b: int, ctx: Any = None) -> dict[str, int]:
            return {"r": a * b}

    Calc.__module__ = mod_name
    Calc = agent(name="calc", version="0.1.0", description="d")(Calc)  # NOSONAR

    inst = Calc()
    ctx = SimpleNamespace(logger=None)

    r1 = asyncio.run(
        inst.__apollia_dispatch__(  # type: ignore[attr-defined]
            {
                "skill_id": "calc.add",
                "input": {"parts": [{"type": "data", "data": {"a": 2, "b": 3}}]},
            },
            ctx,
        )
    )
    r2 = asyncio.run(
        inst.__apollia_dispatch__(  # type: ignore[attr-defined]
            {
                "skill_id": "calc.mul",
                "input": {"parts": [{"type": "data", "data": {"a": 4, "b": 5}}]},
            },
            ctx,
        )
    )
    assert r1["status"] == "completed"
    assert r2["status"] == "completed"


def test_skill_payload_validation_failure() -> None:
    mod_name = _new_module()

    class A:
        @skill("typed.op")
        async def op(self, n: int, ctx: Any = None) -> dict[str, int]:
            return {"n": n}

    A.__module__ = mod_name
    A = agent(name="a", version="0.1.0", description="d")(A)

    # Missing required arg "n".
    result = asyncio.run(
        A().__apollia_dispatch__(  # type: ignore[attr-defined]
            {"skill_id": "typed.op", "input": {"parts": []}},
            SimpleNamespace(logger=None),
        )
    )
    assert result["status"] == "failed"
