"""Tests for ``apollia.cli.inspect`` — the ``apollia inspect`` sub-command."""

from __future__ import annotations

import argparse
import json
import sys
import textwrap
from pathlib import Path

import pytest

from apollia.cli import inspect as inspect_cmd


# ──────────────────────────────────────────────────────────────────────
# Fixtures — temporary agent files
# ──────────────────────────────────────────────────────────────────────


def _write(path: Path, content: str) -> None:
    path.write_text(textwrap.dedent(content), encoding="utf-8")


@pytest.fixture
def modern_agent(tmp_path: Path) -> Path:
    """A minimal ``@agent``-decorated worker with one ``@skill``."""
    path = tmp_path / "modern_agent.py"
    _write(
        path,
        '''
        """A toy modern agent for inspect tests.

        Imports the ``@agent`` / ``@skill`` decorators under aliased names so
        that the module-level ``agent`` symbol can be exclusively owned by
        the singleton instance produced by the decorator.
        """

        from __future__ import annotations

        from typing import Any

        from apollia.agent import agent as _agent
        from apollia.skills import skill as _skill


        @_agent(
            name="toy-worker",
            version="0.1.0",
            description="Toy worker for testing apollia inspect.",
            tags=("test", "toy"),
            packages=("toy-pkg",),
        )
        class ToyWorker:
            @_skill("toy.echo", description="Echo back the input message.")
            async def echo(self, ctx: Any, message: str) -> dict[str, Any]:
                return {"reply": message}
        ''',
    )
    return path


@pytest.fixture
def legacy_agent(tmp_path: Path) -> Path:
    """A legacy agent: top-level ``manifest()`` function + ``agent`` instance
    whose class is *not* decorated with ``@agent``."""
    path = tmp_path / "legacy_agent.py"
    _write(
        path,
        '''
        """A legacy-style worker (no @agent decorator)."""

        from __future__ import annotations


        def manifest() -> dict:
            return {
                "name": "legacy-worker",
                "version": "0.0.1",
                "description": "Legacy worker for tests.",
                "tags": ["legacy"],
                "packages": [],
                "datasources": [],
                "templates": [],
                "secrets": [],
                "tools_required": [],
                "execution_mode": "direct",
                "supports_a2a": True,
                "skills": [
                    {
                        "id": "legacy.run",
                        "name": "legacy.run",
                        "description": "Run the legacy worker.",
                    }
                ],
            }


        class LegacyWorker:
            def manifest(self) -> dict:
                return manifest()


        agent = LegacyWorker()
        ''',
    )
    return path


@pytest.fixture
def broken_agent(tmp_path: Path) -> Path:
    """A module that raises at import time."""
    path = tmp_path / "broken_agent.py"
    _write(
        path,
        '''
        """This module fails on import."""

        raise RuntimeError("boom during import")
        ''',
    )
    return path


@pytest.fixture
def no_agent_module(tmp_path: Path) -> Path:
    """A module that loads cleanly but exposes no agent / manifest."""
    path = tmp_path / "no_agent.py"
    _write(
        path,
        '''
        """Module without any agent surface."""

        VALUE = 42
        ''',
    )
    return path


def _make_args(path: Path, *, json_mode: bool = False) -> argparse.Namespace:
    return argparse.Namespace(agent_path=str(path), json=json_mode)


# ──────────────────────────────────────────────────────────────────────
# Happy path — modern @agent module
# ──────────────────────────────────────────────────────────────────────


class TestModernAgent:
    """Inspection of a class decorated with ``@agent``."""

    def test_exit_code_zero(
        self, modern_agent: Path, capsys: pytest.CaptureFixture[str]
    ) -> None:
        """GIVEN a valid modern agent WHEN inspect runs THEN exit code is 0."""
        rc = inspect_cmd.inspect_command(_make_args(modern_agent))
        assert rc == 0

    def test_human_output_contains_name(
        self, modern_agent: Path, capsys: pytest.CaptureFixture[str]
    ) -> None:
        """Human report mentions name, version, and skill id."""
        rc = inspect_cmd.inspect_command(_make_args(modern_agent))
        out = capsys.readouterr().out
        assert rc == 0
        assert "toy-worker" in out
        assert "0.1.0" in out
        assert "toy.echo" in out
        # Marker for the validation block.
        assert "Status: valid" in out

    def test_skill_block_renders(
        self, modern_agent: Path, capsys: pytest.CaptureFixture[str]
    ) -> None:
        """Skills section shows at least one bullet with the skill id."""
        inspect_cmd.inspect_command(_make_args(modern_agent))
        out = capsys.readouterr().out
        assert "Skills (1)" in out
        assert "• toy.echo" in out
        assert "Echo back the input message." in out

    def test_json_output_is_valid(
        self, modern_agent: Path, capsys: pytest.CaptureFixture[str]
    ) -> None:
        """``--json`` emits a parseable JSON document with the right keys."""
        rc = inspect_cmd.inspect_command(_make_args(modern_agent, json_mode=True))
        assert rc == 0
        out = capsys.readouterr().out
        payload = json.loads(out)
        assert payload["manifest"]["name"] == "toy-worker"
        assert payload["manifest"]["version"] == "0.1.0"
        assert payload["manifest"]["supports_a2a"] is True
        assert {s["id"] for s in payload["skills"]} == {"toy.echo"}
        assert payload["errors"] == []
        # No spurious warnings for a fully-decorated agent.
        assert payload["warnings"] == []


# ──────────────────────────────────────────────────────────────────────
# Legacy fallback
# ──────────────────────────────────────────────────────────────────────


class TestLegacyAgent:
    """Inspection of a non-decorated agent with a legacy ``manifest()``."""

    def test_legacy_succeeds_with_warning(
        self, legacy_agent: Path, capsys: pytest.CaptureFixture[str]
    ) -> None:
        """Exit code 0 + at least one warning in JSON output."""
        rc = inspect_cmd.inspect_command(_make_args(legacy_agent, json_mode=True))
        assert rc == 0
        payload = json.loads(capsys.readouterr().out)
        assert payload["manifest"]["name"] == "legacy-worker"
        assert payload["warnings"], "expected at least one warning"
        # The legacy manifest declares one skill; extraction should preserve it.
        assert any(s["id"] == "legacy.run" for s in payload["skills"])


# ──────────────────────────────────────────────────────────────────────
# Failure paths
# ──────────────────────────────────────────────────────────────────────


class TestFailureModes:
    """Error scenarios: missing file, wrong suffix, broken module, etc."""

    def test_missing_file_returns_two(
        self, tmp_path: Path, capsys: pytest.CaptureFixture[str]
    ) -> None:
        """A non-existent path yields exit code 2."""
        missing = tmp_path / "does_not_exist.py"
        rc = inspect_cmd.inspect_command(_make_args(missing))
        assert rc == 2
        err = capsys.readouterr().err
        assert "File not found" in err

    def test_non_python_file_returns_two(
        self, tmp_path: Path, capsys: pytest.CaptureFixture[str]
    ) -> None:
        """A ``.txt`` file is rejected with exit code 2."""
        txt = tmp_path / "agent.txt"
        txt.write_text("not python", encoding="utf-8")
        rc = inspect_cmd.inspect_command(_make_args(txt))
        assert rc == 2
        err = capsys.readouterr().err
        assert ".py" in err

    def test_broken_module_returns_one(
        self, broken_agent: Path, capsys: pytest.CaptureFixture[str]
    ) -> None:
        """A module raising at import time yields exit code 1."""
        rc = inspect_cmd.inspect_command(_make_args(broken_agent))
        assert rc == 1
        err = capsys.readouterr().err
        assert "Failed to load module" in err
        assert "boom during import" in err

    def test_module_without_agent_returns_one(
        self, no_agent_module: Path, capsys: pytest.CaptureFixture[str]
    ) -> None:
        """A loadable module with no agent surface fails with exit code 1."""
        rc = inspect_cmd.inspect_command(_make_args(no_agent_module))
        assert rc == 1
        err = capsys.readouterr().err
        assert "Inspection failed" in err

    def test_json_mode_emits_errors_array_on_failure(
        self, broken_agent: Path, capsys: pytest.CaptureFixture[str]
    ) -> None:
        """In JSON mode, load failures land in ``errors`` not stderr text."""
        rc = inspect_cmd.inspect_command(
            _make_args(broken_agent, json_mode=True)
        )
        assert rc == 1
        out = capsys.readouterr().out
        payload = json.loads(out)
        assert payload["errors"]
        assert any("boom" in e or "Failed to load" in e for e in payload["errors"])


# ──────────────────────────────────────────────────────────────────────
# Schema formatter
# ──────────────────────────────────────────────────────────────────────


class TestSchemaBrief:
    """Unit tests for the JSON-Schema → one-line formatter."""

    def test_object_schema_renders_required_marker(self) -> None:
        schema = {
            "type": "object",
            "properties": {
                "path": {"type": "string"},
                "max_chars": {"type": "integer"},
            },
            "required": ["path"],
        }
        rendered = inspect_cmd._format_schema_brief(schema)
        assert "path: string!" in rendered
        assert "max_chars: integer?" in rendered

    def test_empty_schema_returns_none(self) -> None:
        assert inspect_cmd._format_schema_brief({}) == "(none)"

    def test_non_dict_returns_any(self) -> None:
        assert inspect_cmd._format_schema_brief(None) == "(any)"


# ──────────────────────────────────────────────────────────────────────
# Parser wiring
# ──────────────────────────────────────────────────────────────────────


def test_build_parser_registers_inspect() -> None:
    """``build_parser`` adds an ``inspect`` sub-command with ``--json``."""
    root = argparse.ArgumentParser()
    subs = root.add_subparsers(dest="command")
    inspect_cmd.build_parser(subs)
    args = root.parse_args(["inspect", "/tmp/some.py", "--json"])
    assert args.command == "inspect"
    assert args.agent_path == "/tmp/some.py"
    assert args.json is True
    assert args.func is inspect_cmd.inspect_command


def test_main_dispatcher_inspect_unknown_path(
    tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    """Top-level ``main`` dispatches ``inspect`` and returns its exit code."""
    from apollia.cli.__main__ import main as cli_main

    missing = tmp_path / "missing.py"
    rc = cli_main(["inspect", str(missing)])
    assert rc == 2
