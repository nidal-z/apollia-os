"""Host-side demo: drive a real Apollia daemon through the generated Python SDK.

Proves the driving contract end to end over TCP + bearer token: read the token
the daemon generated, open an authenticated client, launch the `echo` workflow
of the installed `echo` package, poll the run to its end, and print the result.
A workflow runs its steps as written, so no model is involved. No hand-written
HTTP; every call goes through the generated `apollia_runtime_client`.
"""

# REASON: print-call: this module is the command-line entry point of the demo.

import os
import pathlib
import sys
import time

# Make the generated client importable without packaging it (meta = none).
_HERE = pathlib.Path(__file__).resolve()
sys.path.insert(0, str(_HERE.parents[1] / "python"))

from apollia_runtime_client import AuthenticatedClient
from apollia_runtime_client.api.health import health_handler
from apollia_runtime_client.api.workflows import get_workflow_run, launch_workflow
from apollia_runtime_client.models import (
    WorkflowLaunch,
    WorkflowLaunchInputs,
    WorkflowRunDetailView,
    WorkflowRunView,
)

BASE_URL = f"http://127.0.0.1:{os.environ.get('APOLLIA_TCP_PORT', '7771')}"
TOKEN = (pathlib.Path.home() / ".apollia" / "api-token").read_text().strip()
MESSAGE = "hello from the host SDK"


def main() -> int:
    client = AuthenticatedClient(base_url=BASE_URL, token=TOKEN)

    health = health_handler.sync(client=client)
    print(f"[1] GET  /api/v1/health                   -> {health.status}")

    launched = launch_workflow.sync(
        "echo",
        "echo",
        client=client,
        body=WorkflowLaunch(inputs=WorkflowLaunchInputs.from_dict({"message": MESSAGE})),
    )
    if not isinstance(launched, WorkflowRunView):
        print(f"[2] POST /api/v1/workflows/echo/echo/runs -> refused: {launched}")
        print("DEMO FAILED")
        return 1
    print(f"[2] POST /api/v1/workflows/echo/echo/runs -> {launched.run_id} ({launched.status})")

    run = launched
    for _ in range(120):
        detail = get_workflow_run.sync(launched.run_id, client=client)
        if isinstance(detail, WorkflowRunDetailView):
            run = detail.run
            if run.status != "running":
                break
        time.sleep(0.5)

    print(f"[3] GET  /api/v1/workflow-runs/{{id}}       -> {run.status}")
    output = run.output.to_dict() if hasattr(run.output, "to_dict") else run.output
    print(f"[4] result                                -> {output}")

    ok = run.status == "completed" and output == {"echo": MESSAGE}
    print("DEMO OK" if ok else "DEMO FAILED")
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
