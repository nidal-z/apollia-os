"""End-to-end: a workflow pauses on a question, against a real `apollia-os` daemon.

What is proved, on a throwaway HOME, through the public HTTP API and the CLI:

1. A workflow of an installed package asks the user from inside a step
   (``ctx.user.ask``): the run waits in that step, and
   ``GET /api/v1/pending-decisions`` lists the question with the run, the
   workflow and the step it comes from, its choices, and when it was asked.
2. An answer the question does not take is a 422 with its typed code and
   leaves the run paused.
3. ``POST /api/v1/pending-decisions/decide`` answers it: the run goes on in
   the step that asked, asks a second question built from the first answer,
   and returns what it received across both. A free answer outside the
   choices is taken.
4. A declined question reaches the workflow as ``QuestionDeclined`` with the
   reason; left uncaught, it fails the step and the run.
5. ``apollia-os approvals pending`` shows the paused run and
   ``apollia-os approvals decide`` answers it.
6. A run that waits when the runtime stops is failed at the next start, with
   its reason, and no longer listed.

The package is assembled in the throwaway HOME from ``hitl_e2e_agent.py``:
``python -m apollia describe`` writes its workflow descriptor, ``apollia-os
package build`` and ``package install`` do the rest.

Needs a built daemon, the ``apollia-os`` binary at ``target/debug`` (``just
build-cli``) or at ``APOLLIA_E2E_BIN``. Without one the module is skipped and
says it measured nothing. Stdlib only, like every agent.
"""

import json
import os
import shutil
import socket
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request
from collections.abc import Iterator
from pathlib import Path
from typing import Any

import pytest

REPO = Path(__file__).resolve().parents[2]
MODULE = Path(__file__).resolve().parent / "hitl_e2e_agent.py"
BINARY = Path(os.environ.get("APOLLIA_E2E_BIN", REPO / "target" / "debug" / "apollia-os"))

pytestmark = pytest.mark.skipif(
    not BINARY.is_file(),
    reason=f"no apollia-os binary at {BINARY}: nothing measured, run `just build-cli`",
)

DEADLINE_SECS = 60.0
WORKFLOW = "hitl-e2e/purge"
FIRST = "Which list should be purged?"

MANIFEST = """[package]
name = "hitl-e2e"
version = "1.0.0"
publisher = "Apollia tests"
description = "A workflow that pauses on questions."
sdk = ">=0.3"
"""


def _free_port() -> int:
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return int(s.getsockname()[1])


class Daemon:
    """An `apollia-os start` process on its own HOME and port."""

    def __init__(self, home: Path, port: int) -> None:
        self.home = home
        self.port = port
        # The SDK of this tree goes first on the interpreter's path, so the
        # workflow imports the `apollia` this checkout holds and not the one
        # the embedded interpreter was bundled with.
        self.env = {**os.environ, "HOME": str(home), "PYTHONPATH": str(REPO / "sdk")}
        self.log = home / "daemon.log"
        self.process: subprocess.Popen[bytes] | None = None
        self.token = ""

    def start(self) -> None:
        with self.log.open("ab") as log:
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
            self.process.wait(timeout=30)

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

    def cli(self, *args: str) -> subprocess.CompletedProcess[str]:
        """Run `apollia-os` against this daemon, as its user would."""
        return subprocess.run(
            [str(BINARY), *args],
            cwd=self.home,
            env=self.env,
            capture_output=True,
            text=True,
            timeout=120,
        )


def install_package(d: Daemon) -> None:
    """Build the package `hitl-e2e` from the workflow module and install it."""
    folder = d.home / "hitl-e2e"
    folder.mkdir()
    (folder / "apollia.toml").write_text(MANIFEST)
    shutil.copy(MODULE, folder / MODULE.name)
    described = subprocess.run(
        [sys.executable, "-m", "apollia", "describe", str(folder)],
        env=d.env,
        capture_output=True,
        text=True,
        timeout=120,
    )
    assert described.returncode == 0, described.stdout + described.stderr
    archive = d.home / "hitl-e2e.apo"
    built = d.cli("package", "build", str(folder), "-o", str(archive))
    assert built.returncode == 0, built.stdout + built.stderr
    installed = d.cli("package", "install", str(archive), "--json")
    assert installed.returncode == 0, installed.stdout + installed.stderr


@pytest.fixture(scope="module")
def daemon() -> Iterator[Daemon]:
    home = Path(tempfile.mkdtemp(prefix="apollia-hitl-e2e-"))
    d = Daemon(home, _free_port())
    try:
        d.start()
        install_package(d)
        # The workflows of the installed packages are handed to the engine
        # when the runtime starts: a package installed under a running
        # runtime is launchable from its next start.
        d.stop()
        d.start()
        yield d
    finally:
        d.stop()
        shutil.rmtree(home, ignore_errors=True)


def launch(daemon: Daemon) -> str:
    status, body = daemon.request(
        "POST", "/api/v1/workflows/hitl-e2e/purge/runs", {"inputs": {"crm": "acme"}}
    )
    assert status in (200, 201, 202), body
    return str(body["runId"])


def pending(daemon: Daemon, run_id: str) -> list[dict[str, Any]]:
    """The decisions waiting that come from the run `run_id`."""
    status, body = daemon.request("GET", "/api/v1/pending-decisions")
    assert status == 200, body
    return [i for i in body["items"] if i["origin"].get("runId") == run_id]


def wait_paused(daemon: Daemon, run_id: str, question: str) -> dict[str, Any]:
    """The listed item of `run_id` once it waits on `question`."""
    deadline = time.monotonic() + DEADLINE_SECS
    last: Any = None
    while time.monotonic() < deadline:
        for item in pending(daemon, run_id):
            last = item
            if item["summary"] == question:
                return item
        time.sleep(0.2)
    raise AssertionError(f"run {run_id} never paused on {question!r}; last listed: {last}")


def decide(daemon: Daemon, item: dict[str, Any], **answer: Any) -> tuple[int, Any]:
    return daemon.request("POST", "/api/v1/pending-decisions/decide", {"id": item["id"], **answer})


def read_run(daemon: Daemon, run_id: str) -> dict[str, Any]:
    status, body = daemon.request("GET", f"/api/v1/workflow-runs/{run_id}")
    assert status == 200, body
    return dict(body)


def wait_ended(daemon: Daemon, run_id: str) -> dict[str, Any]:
    """The run `run_id` with its steps, once it is no longer running."""
    deadline = time.monotonic() + DEADLINE_SECS
    body: dict[str, Any] = {}
    while time.monotonic() < deadline:
        body = read_run(daemon, run_id)
        if body["run"]["status"] != "running":
            return body
        time.sleep(0.2)
    raise AssertionError(f"run {run_id} never ended; last: {body}")


def step_statuses(detail: dict[str, Any]) -> dict[str, str]:
    return {s["name"]: s["status"] for s in detail["steps"]}


def test_two_questions_answered_in_turn_return_both_answers(daemon: Daemon) -> None:
    # GIVEN a run of the workflow that asks which list, then a confirmation
    run_id = launch(daemon)

    # WHEN it pauses on the first question
    first = wait_paused(daemon, run_id, FIRST)

    # THEN the listing says what is asked, by which workflow, in which step,
    #      when, with the choices proposed and a free answer besides them
    assert first["kind"] == "question"
    assert first["origin"]["workflowId"] == WORKFLOW
    assert first["origin"]["step"] == "choose"
    assert first["id"] == f"workflow-question:{first['origin']['requestId']}"
    assert first["askedAt"]
    assert first["answers"] == ["answer", "refuse"]
    assert first["allowOther"] is True
    assert first["questions"] == [
        {
            "id": "answer",
            "question": FIRST,
            "type": "single_choice",
            "options": ["leads", "clients"],
            "hint": "a list of the CRM",
        }
    ]
    # AND the run waits inside the step that asked
    waiting = read_run(daemon, run_id)
    assert waiting["run"]["status"] == "running"
    assert step_statuses(waiting) == {
        "choose": "running",
        "confirm": "pending",
        "purge": "pending",
    }

    # WHEN it is given an answer a question does not take, then an empty one
    status, body = decide(daemon, first, answer="once")
    # THEN each is refused with its typed code and the run stays paused
    assert status == 422, body
    assert body["code"] == "ANSWER_NOT_OFFERED"
    status, body = decide(daemon, first, answer="answer", value="")
    assert status == 422, body
    assert body["code"] == "INVALID_ANSWER"
    assert wait_paused(daemon, run_id, FIRST)["id"] == first["id"]

    # WHEN the user picks a choice
    status, body = decide(daemon, first, answer="answer", value="clients")
    assert status == 200, body
    assert body["resumed"] == 1

    # THEN the run, resumed, pauses again on the question it built from the answer
    second = wait_paused(daemon, run_id, "Purge the list clients of acme?")
    assert second["origin"]["step"] == "confirm"
    assert second["questions"][0]["options"] == ["yes", "no"]
    assert second["id"] != first["id"]

    # WHEN the first question is answered again, then the second one properly
    status, body = decide(daemon, first, answer="answer", value="leads")
    assert status == 404, body
    assert body["code"] == "UNKNOWN_ITEM"
    status, body = decide(daemon, second, answer="answer", value="yes")
    assert status == 200, body

    # THEN the run completes, every step done, with what it received across both
    ended = wait_ended(daemon, run_id)
    assert ended["run"]["status"] == "completed", ended
    assert ended["run"]["output"] == {"list": "clients", "purged": True, "confirmation": "yes"}
    assert step_statuses(ended) == {
        "choose": "completed",
        "confirm": "completed",
        "purge": "completed",
    }
    assert pending(daemon, run_id) == []


def test_a_free_answer_is_taken_and_a_declined_question_is_a_typed_refusal(
    daemon: Daemon,
) -> None:
    # GIVEN a run paused on its first question
    run_id = launch(daemon)
    first = wait_paused(daemon, run_id, FIRST)

    # WHEN the user answers with a list that is not proposed
    status, body = decide(daemon, first, answer="answer", value="prospects")
    assert status == 200, body

    # THEN the workflow goes on with the free answer
    second = wait_paused(daemon, run_id, "Purge the list prospects of acme?")

    # WHEN the user declines the confirmation, saying why
    status, body = decide(daemon, second, answer="refuse", reason="not on production")
    assert status == 200, body

    # THEN the workflow caught QuestionDeclined, naming the question and the reason
    ended = wait_ended(daemon, run_id)
    assert ended["run"]["status"] == "completed", ended
    assert ended["run"]["output"] == {
        "list": "prospects",
        "purged": False,
        "declined_question": "Purge the list prospects of acme?",
        "decline_reason": "not on production",
    }


def test_a_declined_question_left_uncaught_fails_the_run_in_its_step(daemon: Daemon) -> None:
    # GIVEN a run paused on the question it does not guard
    run_id = launch(daemon)
    first = wait_paused(daemon, run_id, FIRST)

    # WHEN the user declines it
    status, body = decide(daemon, first, answer="refuse", reason="wrong CRM")
    assert status == 200, body

    # THEN the step that asked failed with the refusal, and the next never started
    ended = wait_ended(daemon, run_id)
    assert ended["run"]["status"] == "failed", ended
    assert "QuestionDeclined" in ended["run"]["error"]
    assert "wrong CRM" in ended["run"]["error"]
    assert step_statuses(ended) == {
        "choose": "failed",
        "confirm": "not_started",
        "purge": "not_started",
    }
    assert pending(daemon, run_id) == []


def test_the_cli_shows_a_paused_run_and_answers_it(daemon: Daemon) -> None:
    # GIVEN a run paused on its first question
    run_id = launch(daemon)
    first = wait_paused(daemon, run_id, FIRST)

    # WHEN the decisions waiting are listed from the command line
    listed = daemon.cli("approvals", "pending", "--json")
    human = daemon.cli("approvals", "pending")

    # THEN the paused run is there, in JSON and in the table, with its id
    assert listed.returncode == 0, listed.stdout + listed.stderr
    mine = [i for i in json.loads(listed.stdout)["items"] if i["origin"].get("runId") == run_id]
    assert [i["id"] for i in mine] == [first["id"]]
    assert human.returncode == 0, human.stdout + human.stderr
    assert FIRST in human.stdout
    assert first["id"] in human.stdout + human.stderr

    # WHEN an answer is given with no value, then with one
    empty = daemon.cli("approvals", "decide", first["id"], "answer")
    answered = daemon.cli(
        "approvals", "decide", first["id"], "answer", "--value", "leads", "--json"
    )

    # THEN the first is refused and the second resumes the run
    assert empty.returncode == 1, empty.stdout + empty.stderr
    assert answered.returncode == 0, answered.stdout + answered.stderr
    assert json.loads(answered.stdout)["resumed"] == 1
    second = wait_paused(daemon, run_id, "Purge the list leads of acme?")

    # WHEN the confirmation is declined from the command line, then declined again
    declined = daemon.cli("approvals", "decide", second["id"], "refuse", "--reason", "too late")
    again = daemon.cli("approvals", "decide", second["id"], "refuse")

    # THEN the workflow was told once, and the item that no longer waits exits 1
    assert declined.returncode == 0, declined.stdout + declined.stderr
    assert again.returncode == 1, again.stdout + again.stderr
    ended = wait_ended(daemon, run_id)
    assert ended["run"]["output"]["decline_reason"] == "too late"
    status = daemon.cli("workflow", "status", run_id, "--json")
    assert status.returncode == 0, status.stdout + status.stderr
    assert json.loads(status.stdout)["run"]["status"] == "completed"


def test_a_run_paused_when_the_runtime_stops_is_failed_at_the_next_start(
    daemon: Daemon,
) -> None:
    # GIVEN a run paused on its first question
    run_id = launch(daemon)
    wait_paused(daemon, run_id, FIRST)

    # WHEN the runtime stops and starts again on the same HOME
    daemon.stop()
    daemon.start()

    # THEN the run is failed with a reason a user can read, nothing waits for
    #      it any more, and the workflow can be launched again
    ended = read_run(daemon, run_id)
    assert ended["run"]["status"] == "failed", ended
    assert ended["run"]["error"] == "the runtime stopped during the run", ended
    assert step_statuses(ended) == {
        "choose": "failed",
        "confirm": "not_started",
        "purge": "not_started",
    }
    assert pending(daemon, run_id) == []
    again = launch(daemon)
    wait_paused(daemon, again, FIRST)
    status, body = daemon.request("POST", f"/api/v1/workflow-runs/{again}/cancel")
    assert status == 200, body
    assert pending(daemon, again) == []
