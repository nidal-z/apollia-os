# apollia-runtime-client (Python)

Typed client a host application uses to drive an Apollia runtime over its HTTP
API. Generated from the runtime's OpenAPI spec with `openapi-python-client`
(one module per operation, one model per schema).

Runtime deps: `httpx`, `attrs`, `python-dateutil`.

## Usage

```python
import pathlib
from apollia_runtime_client import AuthenticatedClient
from apollia_runtime_client.api.tasks import submit_task, get_task
from apollia_runtime_client.models import SubmitTaskRequest, SubmitTaskRequestInput

token = (pathlib.Path.home() / ".apollia" / "api-token").read_text().strip()
client = AuthenticatedClient(base_url="http://127.0.0.1:7771", token=token)

resp = submit_task.sync(client=client, body=SubmitTaskRequest(
    agent_id="echo",
    input_=SubmitTaskRequestInput.from_dict(
        {"parts": [{"type": "text", "text": "hello"}]}
    ),
))
task = get_task.sync(id=resp.task_id, client=client)
print(task.status, task.result)
```

A full end-to-end example lives in `clients/examples/demo_driver.py`, driven by
`clients/examples/demo_python.sh`.

## Regenerate

```sh
bash clients/regen.sh            # from committed clients/openapi.json
bash clients/regen.sh --from-daemon   # refresh spec from a running daemon first
```

## Known gaps

Three endpoints are not generated as client methods because they take a raw
(non-JSON-schema) request body: `PUT /api/v1/stt/config`,
`POST /api/v1/stt/transcribe`, and `POST /webhooks/{id}`. They remain documented
in the spec; call them directly with `httpx` if needed.
