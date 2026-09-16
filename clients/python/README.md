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

## Raw request bodies

Two operations take a body that is not JSON. `transcribe_audio` sends a
multipart form (`audio` as a WAV file, `language` optional) and
`handle_webhook` sends raw bytes. A webhook is authenticated by an
`X-Apollia-Signature` HMAC-SHA256 header over those bytes rather than by the
API token, so the caller computes the signature and sets the header with
`Client.with_headers` before sending.
