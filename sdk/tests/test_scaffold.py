"""Tests for the ``apollia new`` scaffolding command (decorator SDK)."""

from __future__ import annotations

import os

import pytest

from apollia.cli.scaffold import (
    scaffold_agent,
    to_class_name,
    to_module_name,
)


class TestNameConversions:
    """Verify kebab-case ↔ PascalCase / snake_case helpers."""

    @pytest.mark.parametrize(
        ("input_name", "expected"),
        [
            ("hello", "HelloAgent"),
            ("my-agent", "MyAgent"),
            ("chat-bot", "ChatBotAgent"),
            ("a-b-c", "ABCAgent"),
            ("already_snake", "AlreadySnakeAgent"),
            ("review-agent", "ReviewAgent"),
        ],
    )
    def test_to_class_name(self, input_name: str, expected: str) -> None:
        assert to_class_name(input_name) == expected

    @pytest.mark.parametrize(
        ("input_name", "expected"),
        [
            ("hello", "hello"),
            ("my-agent", "my_agent"),
            ("chat-bot", "chat_bot"),
        ],
    )
    def test_to_module_name(self, input_name: str, expected: str) -> None:
        assert to_module_name(input_name) == expected


class TestScaffoldAgent:
    """Verify file generation for each agent type."""

    def test_scaffold_react_agent(self, tmp_path: str) -> None:
        agent_path, test_path = scaffold_agent(
            "hello", agent_type="react", output_dir=str(tmp_path),
        )

        assert os.path.isfile(agent_path)
        assert os.path.isfile(test_path)
        assert os.path.basename(agent_path) == "hello_agent.py"
        assert os.path.basename(test_path) == "test_hello_agent.py"

        agent_src = open(agent_path, encoding="utf-8").read()
        assert "@agent(" in agent_src
        assert "class HelloAgent:" in agent_src
        assert '"name": "hello"' not in agent_src
        # The decorator carries the name; templates simply embed the literal.
        assert 'name="hello"' in agent_src
        assert "from apollia import" in agent_src

        test_src = open(test_path, encoding="utf-8").read()
        assert "from hello_agent import HelloAgent" in test_src
        assert "from apollia.testing import mock" in test_src

    def test_scaffold_conversational_agent(self, tmp_path: str) -> None:
        agent_path, _test_path = scaffold_agent(
            "chat-bot", agent_type="conversational", output_dir=str(tmp_path),
        )

        assert os.path.isfile(agent_path)
        assert os.path.basename(agent_path) == "chat_bot_agent.py"

        agent_src = open(agent_path, encoding="utf-8").read()
        assert "class ChatBotAgent:" in agent_src
        assert "@on_message" in agent_src
        assert 'name="chat-bot"' in agent_src

    def test_scaffold_orchestrated_agent(self, tmp_path: str) -> None:
        agent_path, _test_path = scaffold_agent(
            "planner", agent_type="orchestrated", output_dir=str(tmp_path),
        )

        agent_src = open(agent_path, encoding="utf-8").read()
        assert "class PlannerAgent:" in agent_src
        assert "@orchestrated(system_prompt=" in agent_src
        assert "on_plan_complete" in agent_src

    def test_scaffold_invalid_type_raises(self, tmp_path: str) -> None:
        with pytest.raises(ValueError, match="Invalid agent type 'invalid'"):
            scaffold_agent("test", agent_type="invalid", output_dir=str(tmp_path))

    def test_scaffold_file_exists_raises(self, tmp_path: str) -> None:
        scaffold_agent("dup", agent_type="react", output_dir=str(tmp_path))

        with pytest.raises(FileExistsError, match="already exists"):
            scaffold_agent("dup", agent_type="react", output_dir=str(tmp_path))

    def test_scaffold_creates_output_dir(self, tmp_path: str) -> None:
        nested = os.path.join(str(tmp_path), "sub", "dir")
        agent_path, _test_path = scaffold_agent(
            "nested", agent_type="react", output_dir=nested,
        )
        assert os.path.isfile(agent_path)

    def test_generated_agent_is_valid_python(self, tmp_path: str) -> None:
        for agent_type in ("react", "conversational", "orchestrated"):
            sub = os.path.join(str(tmp_path), agent_type)
            agent_path, test_path = scaffold_agent(
                f"check-{agent_type}",
                agent_type=agent_type,
                output_dir=sub,
            )
            agent_src = open(agent_path, encoding="utf-8").read()
            test_src = open(test_path, encoding="utf-8").read()
            compile(agent_src, agent_path, "exec")
            compile(test_src, test_path, "exec")


class TestScaffoldWorkerAgent:
    """Verify file generation for the worker agent type."""

    def test_scaffold_worker_creates_files(self, tmp_path: str) -> None:
        """The scaffolding creates agent and test files at the expected paths."""
        agent_path, test_path = scaffold_agent(
            "test-worker", agent_type="worker", output_dir=str(tmp_path),
        )

        assert os.path.isfile(agent_path)
        assert os.path.isfile(test_path)

        # Agent lands in agents/ subdirectory with the original kebab-case name.
        assert agent_path == str(
            os.path.join(str(tmp_path), "agents", "test-worker.py")
        )
        # Test lands in agents/tests/ with snake_case prefix.
        assert test_path == str(
            os.path.join(str(tmp_path), "agents", "tests", "test_test_worker.py")
        )

    def test_scaffold_worker_agent_content(self, tmp_path: str) -> None:
        """Generated agent file contains the canonical decorator constructs."""
        agent_path, _ = scaffold_agent(
            "test-worker", agent_type="worker", output_dir=str(tmp_path),
        )
        src = open(agent_path, encoding="utf-8").read()

        assert "from apollia import DomainError, agent, skill" in src
        assert "@agent(" in src
        assert "class TestWorkerAgent:" in src
        assert 'agent_type="worker"' in src
        assert "@skill(" in src
        # No legacy `agent = MyClass()` trailer: the @agent decorator
        # auto-instantiates the class (cf. ADR-107).
        assert "agent_instance" not in src

    def test_scaffold_worker_agent_generated_is_valid_python(
        self, tmp_path: str,
    ) -> None:
        """The generated agent file compiles as valid Python.

        We intentionally do not import the module here - that would pull
        in the `apollia` runtime which expects a real PyO3 context. The
        compile + structural checks above are sufficient at scaffold time.
        """
        agent_path, _ = scaffold_agent(
            "test-worker", agent_type="worker", output_dir=str(tmp_path),
        )
        src = open(agent_path, encoding="utf-8").read()
        compile(src, agent_path, "exec")

    def test_scaffold_worker_generated_files_are_valid_python(
        self, tmp_path: str,
    ) -> None:
        """Both generated files are syntactically valid Python."""
        agent_path, test_path = scaffold_agent(
            "my-domain", agent_type="worker", output_dir=str(tmp_path),
        )
        compile(open(agent_path, encoding="utf-8").read(), agent_path, "exec")
        compile(open(test_path, encoding="utf-8").read(), test_path, "exec")
