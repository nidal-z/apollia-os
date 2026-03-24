"""Tests for the ``apollia new`` scaffolding command."""

from __future__ import annotations

import os
import textwrap

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
        assert "class HelloAgent(BaseReActAgent):" in agent_src
        assert '"name": "hello"' in agent_src
        assert "from apollia.agents import BaseReActAgent" in agent_src

        test_src = open(test_path, encoding="utf-8").read()
        assert "from hello_agent import HelloAgent" in test_src
        assert "MockContext" in test_src
        assert "assert_result_completed" in test_src

    def test_scaffold_conversational_agent(self, tmp_path: str) -> None:
        agent_path, test_path = scaffold_agent(
            "chat-bot", agent_type="conversational", output_dir=str(tmp_path),
        )

        assert os.path.isfile(agent_path)
        assert os.path.basename(agent_path) == "chat_bot_agent.py"

        agent_src = open(agent_path, encoding="utf-8").read()
        assert "class ChatBotAgent(ConversationalAgent):" in agent_src
        assert '"name": "chat-bot"' in agent_src
        assert "from apollia.agents import ConversationalAgent" in agent_src

    def test_scaffold_orchestrated_agent(self, tmp_path: str) -> None:
        agent_path, _test_path = scaffold_agent(
            "planner", agent_type="orchestrated", output_dir=str(tmp_path),
        )

        agent_src = open(agent_path, encoding="utf-8").read()
        assert "class PlannerAgent(OrchestratedAgent):" in agent_src
        assert '"execution_mode": "orchestrated"' in agent_src
        assert "from apollia.agents import OrchestratedAgent" in agent_src

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
