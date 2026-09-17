"""End-to-end: typed pauses on the task path, against a real `apollia-os` daemon.

What is proved, on a throwaway HOME, through the public HTTP API only:

1. An agent installed by ``POST /api/v1/agents`` asks a typed question, then an
   approval, is resumed twice by ``POST /api/v1/tasks/{id}/resume`` with an
   answer, and returns what it received.
2. ``GET /api/v1/tasks?status=input_required`` lists each pause with its agent,
   skill, creation time, prompt and payload.
3. An answer that does not fit the pending pause is a 422 ``INVALID_ANSWER``
   and leaves the task paused.
4. A call to a tool on an MCP server declared ``requires_approval`` pauses the
   task on an ``approbation`` payload naming ``<server>/<tool>`` and its
   arguments; resumed approved, the call runs; resumed declined, the agent
   receives ``ToolApprovalDenied``.

Needs a built daemon, the ``apollia-os`` binary at ``target/debug`` (``just
cli-build``) or at ``APOLLIA_E2E_BIN``. Without one the module is skipped and
says it measured nothing. Stdlib only, like every agent.
"""

import json
import os
import shutil
import socket
import subprocess
import tempfile
import time
import urllib.error
import urllib.request
from collections.abc import Iterator
from pathlib import Path
from typing import Any

import pytest

REPO = Path(__file__).resolve().parents[2]
AGENT = Path(__file__).resolve().parent / "hitl_e2e_agent.py"
MOCK_MCP = REPO / "crates" / "apollia-mcp" / "tests" / "mock_mcp_server.py"
BINARY = Path(os.environ.get("APOLLIA_E2E_BIN", REPO / "target" / "debug" / "apollia-os"))

pytestmark = pytest.mark.skipif(
    not BINARY.is_file(),
    reason=f"no apollia-os binary at {BINARY}: nothing measured, run `just cli-build`",
)

DEADLINE_SECS = 60.0


def _free_port() -> int:
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return int(s.getsockname()[1])


class Daemon:
    """An `apollia-os start` process on its own HOME and port."""

    def __init__(self, home: Path, port: int) -> None:
        self.home = home
        self.port = port
        self.env = {**os.environ, "HOME": str(home)}
        self.log = home / "daemon.log"
        self.process: subprocess.Popen[bytes] | None = None
        self.token = ""

    def start(self) -> None:
        # Eager loading puts every MCP tool descriptor in the registry, which is
        # where install-time resolution reads it. Under the default, deferred,
        # the descriptors never reach the registry and an agent requiring a tool
        # of a gated server installed whatever the descriptor said, so the suite
        # missed the refusal it now holds against.
        (self.home / "apollia.toml").write_text('[mcp]\ntool_loading = "eager"\n')
        with self.log.open("wb") as log:
            self.process = subprocess.Popen(
                [str(BINARY), "start", "--port", str(self.port)],
                cwd=self.home,
                env=self.env,
                stdout=log,
                stderr=subprocess.STDOUT,
            )
        token_file = self.home / ".apollia" / "api-token"
        deadline = time.monotonic() + DEADLINE_SECS
        while time.monotonic() < deadline:
            if self.process.poll() is not None:
                raise AssertionError(f"daemon exited early:\n{self.log.read_text()}")
            if token_file.is_file():
                self.token = token_file.read_text().strip()
                try:
                    status, _ = self.request("GET", "/api/v1/health")
                    if status == 200:
                        return
                except OSError:
                    pass
            time.sleep(0.2)
        raise AssertionError(f"daemon never became healthy:\n{self.log.read_text()}")

    def stop(self) -> None:
        if self.process is None or self.process.poll() is not None:
            return
        subprocess.run([str(BINARY), "stop"], env=self.env, capture_output=True, timeout=30)
        try:
            self.process.wait(timeout=30)
        except subprocess.TimeoutExpired:
            self.process.kill()

    def request(self, method: str, path: str, body: Any = None) -> tuple[int, Any]:
        data = None if body is None else json.dumps(body).encode()
        req = urllib.request.Request(
            f"http://127.0.0.1:{self.port}{path}",
            data=data,
            method=method,
            headers={"Authorization": f"Bearer {self.token}", "Content-Type": "application/json"},
        )
        try:
            with urllib.request.urlopen(req, timeout=30) as resp:
                raw = resp.read()
                return resp.status, json.loads(raw) if raw else None
        except urllib.error.HTTPError as err:
            raw = err.read()
            return err.code, json.loads(raw) if raw else None


@pytest.fixture(scope="module")
def daemon() -> Iterator[Daemon]:
    home = Path(tempfile.mkdtemp(prefix="apollia-hitl-e2e-"))
    d = Daemon(home, _free_port())
    try:
        d.start()
        added = subprocess.run(
            [
                str(BINARY),
                "--json",
                "mcp",
                "add",
                "calc",
                "--command",
                "python3",
                "--arg",
                str(MOCK_MCP),
                "--require-approval",
            ],
            env=d.env,
            capture_output=True,
            text=True,
            timeout=60,
        )
        assert added.returncode == 0, added.stdout + added.stderr
        yield d
    finally:
        d.stop()
        shutil.rmtree(home, ignore_errors=True)


@pytest.fixture(scope="module")
def agent_id(daemon: Daemon) -> str:
    status, body = daemon.request("POST", "/api/v1/agents", {"agent_path": str(AGENT)})
    assert status == 201, body
    return str(body["agent_id"])


def submit(daemon: Daemon, agent: str, skill: str) -> str:
    status, body = daemon.request(
        "POST", "/api/v1/tasks", {"agent_id": agent, "skill_id": skill, "input": {}}
    )
    assert status == 202, body
    return str(body["task_id"])


def wait_paused(daemon: Daemon, task_id: str, genre: str) -> dict[str, Any]:
    """The listed pause of `task_id` once it carries a payload of `genre`."""
    deadline = time.monotonic() + DEADLINE_SECS
    last: Any = None
    while time.monotonic() < deadline:
        status, body = daemon.request("GET", "/api/v1/tasks?status=input_required")
        assert status == 200, body
        for item in body["tasks"]:
            last = item
            if item["task_id"] == task_id and (item.get("payload") or {}).get("genre") == genre:
                return item
        time.sleep(0.2)
    raise AssertionError(f"task {task_id} never paused on a {genre}; last listed: {last}")


def wait_completed(daemon: Daemon, task_id: str) -> Any:
    """The result data of `task_id` once it completed."""
    deadline = time.monotonic() + DEADLINE_SECS
    body: Any = None
    while time.monotonic() < deadline:
        status, body = daemon.request("GET", f"/api/v1/tasks/{task_id}")
        assert status == 200, body
        if body["status"] == "completed":
            return body
        assert body["status"] != "failed", body
        time.sleep(0.2)
    raise AssertionError(f"task {task_id} never completed; last: {body}")


def result_data(body: Any) -> dict[str, Any]:
    """The dict a skill returned, from a completed task's result."""
    result = body["result"]
    text = result if isinstance(result, str) else json.dumps(result)
    return dict(json.loads(text)) if text.strip().startswith("{") else {"raw": text}


def test_question_then_approval_resumed_twice_returns_both_answers(
    daemon: Daemon, agent_id: str
) -> None:
    # GIVEN a task running the skill that asks, then asks for an approval
    task_id = submit(daemon, agent_id, "hitl.ask")

    # WHEN it pauses on the question
    first = wait_paused(daemon, task_id, "choix")

    # THEN the listing says who asks, from which skill, what, and when
    assert first["agent"] == "hitl-e2e-agent"
    assert first["skill"] == "hitl.ask"
    assert first["prompt"] == "Which list should be purged?"
    assert first["created_at"]
    assert [p["id"] for p in first["payload"]["propositions"]] == ["leads", "clients"]

    # WHEN the operator answers with an id that is not proposed
    status, body = daemon.request(
        "POST", f"/api/v1/tasks/{task_id}/resume", {"approved": True, "answer": "prospects"}
    )
    # THEN the answer is refused with the typed code and the task stays paused
    assert status == 422, body
    assert body["code"] == "INVALID_ANSWER"
    wait_paused(daemon, task_id, "choix")

    # WHEN the operator picks a proposition
    status, body = daemon.request(
        "POST", f"/api/v1/tasks/{task_id}/resume", {"approved": True, "answer": "clients"}
    )
    assert status == 200, body

    # THEN the agent, resumed, pauses again on the approval it built from the answer
    second = wait_paused(daemon, task_id, "approbation")
    assert second["payload"]["geste"] == "crm/purge"
    assert second["payload"]["risque"] == "critical"
    assert second["payload"]["detail"] == ["list: clients"]

    # WHEN an approval is answered with an answer, then decided properly
    status, body = daemon.request(
        "POST", f"/api/v1/tasks/{task_id}/resume", {"approved": True, "answer": "yes"}
    )
    assert status == 422, body
    status, body = daemon.request(
        "POST",
        f"/api/v1/tasks/{task_id}/resume",
        {"approved": True, "reason": "checked with sales"},
    )
    assert status == 200, body

    # THEN the task completes and the agent returns what it received across both
    returned = result_data(wait_completed(daemon, task_id))
    assert returned == {
        "is_resumed": True,
        "list": "clients",
        "purge_approved": True,
        "purge_reason": "checked with sales",
        "answered_gesture": "crm/purge",
    }


def test_a_gated_mcp_call_pauses_and_runs_once_approved(daemon: Daemon, agent_id: str) -> None:
    # GIVEN a task calling a tool on a server declared requires_approval
    task_id = submit(daemon, agent_id, "hitl.mcp")

    # WHEN the call is made
    paused = wait_paused(daemon, task_id, "approbation")

    # THEN the task paused on an approval naming the server, the tool and the arguments
    assert paused["payload"]["geste"] == "calc/add"
    assert paused["payload"]["detail"] == ["a: 2", "b: 3"]

    # WHEN it is approved
    status, body = daemon.request("POST", f"/api/v1/tasks/{task_id}/resume", {"approved": True})
    assert status == 200, body

    # THEN the call ran on the real server
    assert result_data(wait_completed(daemon, task_id)) == {"mcp": "5"}


def test_a_declined_mcp_call_reaches_the_agent_as_a_typed_refusal(
    daemon: Daemon, agent_id: str
) -> None:
    # GIVEN a task paused on a gated MCP call
    task_id = submit(daemon, agent_id, "hitl.mcp")
    wait_paused(daemon, task_id, "approbation")

    # WHEN the operator declines it
    status, body = daemon.request(
        "POST",
        f"/api/v1/tasks/{task_id}/resume",
        {"approved": False, "reason": "not on production"},
    )
    assert status == 200, body

    # THEN the agent caught ToolApprovalDenied, naming the gesture and the reason
    assert result_data(wait_completed(daemon, task_id)) == {
        "mcp": "denied",
        "tool": "calc/add",
        "reason": "not on production",
    }


def test_the_engine_pause_keeps_the_context_of_the_resumed_run(
    daemon: Daemon, agent_id: str
) -> None:
    # GIVEN a task paused on the agent's own card, its state in the context
    task_id = submit(daemon, agent_id, "hitl.card_then_write")
    wait_paused(daemon, task_id, "approbation")

    # WHEN the card is approved, and the resumed run makes the gated call
    status, body = daemon.request("POST", f"/api/v1/tasks/{task_id}/resume", {"approved": True})
    assert status == 200, body
    deadline = time.monotonic() + DEADLINE_SECS
    gate: dict[str, Any] = {}
    while time.monotonic() < deadline and gate.get("payload", {}).get("geste") != "calc/add":
        status, pending = daemon.request("GET", "/api/v1/approvals/pending")
        assert status == 200, pending
        gate = next((p for p in pending if p["task_id"] == task_id), {})
        time.sleep(0.2)

    # THEN the approvals list shows the engine's card, typed, and the agent's state
    assert gate["payload"]["geste"] == "calc/add", gate
    assert gate["skill_id"] == "hitl.card_then_write"
    assert gate["context"] == {"records": ["r-1"], "index": 0}

    # WHEN the engine's card is approved
    status, body = daemon.request("POST", f"/api/v1/tasks/{task_id}/resume", {"approved": True})
    assert status == 200, body

    # THEN the agent resumed with its state, and the write ran once
    assert result_data(wait_completed(daemon, task_id)) == {"record": "r-1", "mcp": "5"}
