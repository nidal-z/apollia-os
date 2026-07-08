"""Host-side demo: drive a real Apollia daemon through the generated Python SDK.

Proves the driving contract end to end over TCP + bearer token: read the token
the daemon generated, open an authenticated client, submit a task to the
installed `echo` agent, poll for completion, and print the result. No hand-written
HTTP; every call goes through the generated `apollia_runtime_client`.
"""

import pathlib
import sys
import time

# Make the generated client importable without packaging it (meta = none).
_HERE = pathlib.Path(__file__).resolve()
sys.path.insert(0, str(_HERE.parents[1] / "python"))

from apollia_runtime_client import AuthenticatedClient
from apollia_runtime_client.api.health import health_handler
from apollia_runtime_client.api.tasks import get_task, submit_task
from apollia_runtime_client.models import SubmitTaskRequest, SubmitTaskRequestInput

BASE_URL = "http://127.0.0.1:7771"
TOKEN = (pathlib.Path.home() / ".apollia" / "api-token").read_text().strip()
TERMINAL = {"completed", "succeeded", "done", "failed", "error", "cancelled"}


def main() -> int:
    client = AuthenticatedClient(base_url=BASE_URL, token=TOKEN)

    health = health_handler.sync(client=client)
    print(f"[1] GET  /api/v1/health           -> {health.status}")

    body = SubmitTaskRequest(
        agent_id="echo",
        input_=SubmitTaskRequestInput.from_dict(
            {"parts": [{"type": "text", "text": "hello from the host SDK"}]}
        ),
    )
    submitted = submit_task.sync(client=client, body=body)
    print(f"[2] POST /api/v1/tasks             -> {submitted.task_id} ({submitted.status})")

    task = submitted
    for _ in range(120):
        task = get_task.sync(id=submitted.task_id, client=client)
        if task.status.lower() in TERMINAL or task.result:
            break
        time.sleep(0.5)

    print(f"[3] GET  /api/v1/tasks/{{id}}         -> {task.status}")
    result = task.result.to_dict() if hasattr(task.result, "to_dict") else task.result
    print(f"[4] result                         -> {result}")

    ok = task.status.lower() not in {"failed", "error", "cancelled"}
    print("DEMO OK" if ok else "DEMO FAILED")
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
