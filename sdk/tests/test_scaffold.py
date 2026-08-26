"""Tests for the ``apollia new`` scaffolding command (decorator SDK)."""

from __future__ import annotations

import os
import subprocess
import sys
from pathlib import Path

import pytest
from apollia.cli.scaffold import (
    VALID_AGENT_TYPES,
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
        # GIVEN an agent name in one of the accepted spellings
        # WHEN it is converted to a class name
        # THEN it becomes PascalCase with the Agent suffix, added only once
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
        # GIVEN an agent name in kebab-case or already snake_case
        # WHEN it is converted to a module name
        # THEN it becomes snake_case, importable as written
        assert to_module_name(input_name) == expected


class TestScaffoldAgent:
    """Verify file generation for each agent type."""

    def test_scaffold_react_agent(self, tmp_path: str) -> None:
        # GIVEN a fresh target directory and the react agent type
        # WHEN the agent is scaffolded there
        agent_path, test_path = scaffold_agent(
            "hello",
            agent_type="react",
            output_dir=str(tmp_path),
        )

        # THEN both files land under their expected names and carry the decorator API
        assert os.path.isfile(agent_path)
        assert os.path.isfile(test_path)
        assert os.path.basename(agent_path) == "hello_agent.py"
        assert os.path.basename(test_path) == "test_hello_agent.py"

        agent_src = Path(agent_path).read_text(encoding="utf-8")
        assert "@agent(" in agent_src
        assert "class HelloAgent:" in agent_src
        assert '"name": "hello"' not in agent_src
        # The decorator carries the name; templates simply embed the literal.
        assert 'name="hello"' in agent_src
        assert "from apollia import" in agent_src

        test_src = Path(test_path).read_text(encoding="utf-8")
        assert "from hello_agent import HelloAgent" in test_src
        assert "from apollia.testing import mock" in test_src

    def test_scaffold_conversational_agent(self, tmp_path: str) -> None:
        # GIVEN a fresh target directory and the conversational agent type
        # WHEN the agent is scaffolded there
        agent_path, _test_path = scaffold_agent(
            "chat-bot",
            agent_type="conversational",
            output_dir=str(tmp_path),
        )

        # THEN the generated class carries an @on_message handler and its own name
        assert os.path.isfile(agent_path)
        assert os.path.basename(agent_path) == "chat_bot_agent.py"

        agent_src = Path(agent_path).read_text(encoding="utf-8")
        assert "class ChatBotAgent:" in agent_src
        assert "@on_message" in agent_src
        assert 'name="chat-bot"' in agent_src

    def test_scaffold_orchestrated_agent(self, tmp_path: str) -> None:
        # GIVEN a fresh target directory and the orchestrated agent type
        # WHEN the agent is scaffolded there
        agent_path, _test_path = scaffold_agent(
            "planner",
            agent_type="orchestrated",
            output_dir=str(tmp_path),
        )

        # THEN the generated class carries a system prompt and the plan callback
        agent_src = Path(agent_path).read_text(encoding="utf-8")
        assert "class PlannerAgent:" in agent_src
        assert "@orchestrated(system_prompt=" in agent_src
        assert "on_plan_complete" in agent_src

    def test_scaffold_invalid_type_raises(self, tmp_path: str) -> None:
        # GIVEN an agent type outside the accepted set
        # WHEN the agent is scaffolded
        # THEN it is refused and the message names the offending type
        with pytest.raises(ValueError, match="Invalid agent type 'invalid'"):
            scaffold_agent("test", agent_type="invalid", output_dir=str(tmp_path))

    def test_scaffold_file_exists_raises(self, tmp_path: str) -> None:
        # GIVEN a directory where that agent was already scaffolded once
        scaffold_agent("dup", agent_type="react", output_dir=str(tmp_path))

        # WHEN the same name is scaffolded again
        # THEN it refuses rather than overwriting the author's file
        with pytest.raises(FileExistsError, match="already exists"):
            scaffold_agent("dup", agent_type="react", output_dir=str(tmp_path))

    def test_scaffold_creates_output_dir(self, tmp_path: str) -> None:
        # GIVEN an output directory two levels deep that does not exist
        nested = os.path.join(str(tmp_path), "sub", "dir")
        # WHEN the agent is scaffolded there
        agent_path, _test_path = scaffold_agent(
            "nested",
            agent_type="react",
            output_dir=nested,
        )
        # THEN the tree is created and the file lands in it
        assert os.path.isfile(agent_path)

    def test_generated_agent_is_valid_python(self, tmp_path: str) -> None:
        # GIVEN each of the three in-place agent types
        for agent_type in ("react", "conversational", "orchestrated"):
            sub = os.path.join(str(tmp_path), agent_type)
            # WHEN each is scaffolded and both generated files are compiled
            agent_path, test_path = scaffold_agent(
                f"check-{agent_type}",
                agent_type=agent_type,
                output_dir=sub,
            )
            agent_src = Path(agent_path).read_text(encoding="utf-8")
            test_src = Path(test_path).read_text(encoding="utf-8")
            # THEN neither file has a syntax error
            compile(agent_src, agent_path, "exec")
            compile(test_src, test_path, "exec")


class TestScaffoldWorkerAgent:
    """Verify file generation for the worker agent type."""

    def test_scaffold_worker_creates_files(self, tmp_path: str) -> None:
        """The scaffolding creates agent and test files at the expected paths."""
        # GIVEN a fresh target directory and the worker agent type
        # WHEN the agent is scaffolded there
        agent_path, test_path = scaffold_agent(
            "test-worker",
            agent_type="worker",
            output_dir=str(tmp_path),
        )

        # THEN the two files land at the worker layout's own paths
        assert os.path.isfile(agent_path)
        assert os.path.isfile(test_path)

        # Agent lands in agents/ subdirectory with the original kebab-case name.
        assert agent_path == str(os.path.join(str(tmp_path), "agents", "test-worker.py"))
        # Test lands in agents/tests/ with snake_case prefix.
        assert test_path == str(
            os.path.join(str(tmp_path), "agents", "tests", "test_test_worker.py")
        )

    def test_scaffold_worker_agent_content(self, tmp_path: str) -> None:
        """Generated agent file contains the canonical decorator constructs."""
        # GIVEN a fresh target directory and the worker agent type
        # WHEN the agent is scaffolded and its source is read
        agent_path, _ = scaffold_agent(
            "test-worker",
            agent_type="worker",
            output_dir=str(tmp_path),
        )
        src = Path(agent_path).read_text(encoding="utf-8")

        # THEN it carries the decorator constructs and no legacy instantiation trailer
        assert "from apollia import DomainError, agent, skill" in src
        assert "@agent(" in src
        assert "class TestWorkerAgent:" in src
        assert 'agent_type="worker"' in src
        assert "@skill(" in src
        # No legacy `agent = MyClass()` trailer: the @agent decorator
        # auto-instantiates the class.
        assert "agent_instance" not in src

    def test_scaffold_worker_agent_generated_is_valid_python(
        self,
        tmp_path: str,
    ) -> None:
        """The generated agent file compiles as valid Python.

        We intentionally do not import the module here - that would pull
        in the `apollia` runtime which expects a real PyO3 context. The
        compile + structural checks above are sufficient at scaffold time.
        """
        # GIVEN a scaffolded worker agent
        agent_path, _ = scaffold_agent(
            "test-worker",
            agent_type="worker",
            output_dir=str(tmp_path),
        )
        # WHEN its source is compiled
        src = Path(agent_path).read_text(encoding="utf-8")
        # THEN it has no syntax error
        compile(src, agent_path, "exec")

    def test_scaffold_worker_generated_files_are_valid_python(
        self,
        tmp_path: str,
    ) -> None:
        """Both generated files are syntactically valid Python."""
        # GIVEN a scaffolded worker agent and its generated test
        agent_path, test_path = scaffold_agent(
            "my-domain",
            agent_type="worker",
            output_dir=str(tmp_path),
        )
        # WHEN both sources are compiled
        compile(Path(agent_path).read_text(encoding="utf-8"), agent_path, "exec")
        # THEN neither has a syntax error
        compile(Path(test_path).read_text(encoding="utf-8"), test_path, "exec")


class TestScaffoldedProjectsRun:
    """The generated test suite of every agent type passes as generated.

    Compiling the sources is not enough: the shared test template once
    drove ``invoke_message`` on an ``@orchestrated`` agent that has no
    such handler, and the worker test loaded its module outside
    ``sys.modules``; both compiled cleanly and failed on first run.
    """

    @pytest.mark.integration
    @pytest.mark.parametrize("agent_type", VALID_AGENT_TYPES)
    def test_generated_tests_pass(self, tmp_path: Path, agent_type: str) -> None:
        # GIVEN a freshly scaffolded project of this type
        _agent_path, test_path = scaffold_agent(
            f"my-{agent_type}",
            agent_type=agent_type,
            output_dir=str(tmp_path / "project"),
        )

        # WHEN its generated test file runs under pytest, with a throwaway
        # HOME and the same apollia package this suite imports
        import apollia

        sdk_root = str(Path(apollia.__file__).resolve().parent.parent)
        home = tmp_path / "home"
        home.mkdir()
        env = dict(os.environ, HOME=str(home))
        env["PYTHONPATH"] = os.pathsep.join(p for p in (sdk_root, env.get("PYTHONPATH")) if p)
        # The command is built from sys.executable and a path this test just
        # created; nothing user-controlled reaches it.
        proc = subprocess.run(  # noqa: S603
            [sys.executable, "-m", "pytest", "-q", "-p", "no:cacheprovider", test_path],
            capture_output=True,
            text=True,
            env=env,
            cwd=os.path.dirname(test_path),
            check=False,
        )

        # THEN the generated suite is green
        assert proc.returncode == 0, (
            f"generated {agent_type} tests failed:\n{proc.stdout}\n{proc.stderr}"
        )
