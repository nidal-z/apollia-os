---
sidebar_position: 1
title: Integrate Apollia via the driving contract
description: "Embed an Apollia runtime in your product: the driving contract, authentication, the SDKs, and the calls that start and follow a task."
---

# Integrate Apollia via the driving contract

This guide is for teams embedding an Apollia runtime in a product. It covers the
contract your host application drives: the HTTP API, its stability guarantee,
authentication, and the generated clients. It assumes you can run an Apollia
daemon and want to wire it into an application.

If you just want to see the flow work once, follow
[Drive Apollia from your product](/tutorials/drive-apollia-from-your-product)
first.

## The contract

An Apollia runtime exposes its capabilities over an HTTP API under `/api/v1`:
submit tasks, open chat sessions, stream results, inspect the audit trail, and
manage the runtime.

<!-- claim:daemon-binds-tcp-by-default -->
The daemon started by `apollia-os start` listens on a Unix socket and on
`127.0.0.1:7771`. TCP is bound on every start; `--port` only chooses the number.

The API is described by an OpenAPI 3.1 specification that is generated from the
runtime source, so it cannot drift from the code. The runtime serves it at:

```
GET /api/v1/openapi.json
```

A committed copy also lives at `clients/openapi.json`. For a browsable view of
every operation, see the [HTTP API reference](/reference/api/apollia-os-runtime-api),
which is generated from that same specification.

## Stability guarantee

`/api/v1` is the versioned contract, and the OpenAPI document of each release
describes it exactly. Before 1.0, a release can still remove or change an
operation of `v1`, and each such change is listed with its migration under
"Breaking changes and migration" in the changelog. 0.3.0 removed the project
routes (among them `PUT /api/v1/sessions/{id}/project` and
`GET /api/v1/projects/{id}/sessions`, and the `project` field of
`POST /api/v1/sessions`), the route of the resolved approvals and
`PUT /api/v1/onboarding/profile`. Read that section before you upgrade the
runtime under an integration.

## Authentication

Choose the surface that fits your deployment:

- **Unix socket**: local-trust. Access is governed by filesystem permissions and
  no token is required. Use this when the host and the runtime share a machine
  and a trust boundary.
- **TCP on `127.0.0.1`**: token-authenticated. Every request must carry
  `Authorization: Bearer <token>`. On first boot, when `[api] require_token` is
  enabled (the default), the daemon generates a token and writes it to
  `~/.apollia/api-token` with owner-only permissions. Your host reads that file
  and sends the token.

The runtime the desktop application embeds binds both surfaces too: the Unix
socket, and a TCP port on `127.0.0.1` (`[api] port`, 7771 by default) where the
token is enforced.

## Use a generated client

Two host-driving clients are generated from the OpenAPI specification and live
under `clients/`. Both are generated, not hand-written, so they stay in sync
with the contract. Do not hand-edit generated files; change the runtime and
regenerate.

| Client | Path | Toolchain |
|---|---|---|
| TypeScript | `clients/ts` | `openapi-typescript` types plus `openapi-fetch` |
| Python | `clients/python` | `openapi-python-client` |

Neither is published to a registry, so there is no install command for either.
Both are consumed from a checkout of the repository. `@apollia/runtime-client`
declares its entry point as its own TypeScript sources, so depend on
`clients/ts` by path (a `file:` dependency, or a workspace entry) and let your
bundler compile it; it pulls `openapi-fetch`. `clients/python` is generated with `--meta none`, so it carries
no `pyproject.toml` and `pip install` has nothing to read: put its parent
directory on `PYTHONPATH` or copy `clients/python/apollia_runtime_client` into
your project, and install `httpx` and `attrs` yourself.

### TypeScript

```ts
import { readFileSync } from "node:fs";
import { homedir } from "node:os";
import { createApolliaClient } from "@apollia/runtime-client";

const token = readFileSync(`${homedir()}/.apollia/api-token`, "utf8").trim();
const apollia = createApolliaClient({ token }); // baseUrl defaults to 127.0.0.1:7771

const health = await apollia.GET("/api/v1/health");

const submit = await apollia.POST("/api/v1/tasks", {
  body: {
    agent_id: "coach",
    input: { parts: [{ type: "text", text: "hello from the host" }] },
  },
});
```

Every operation is typed against the contract, so request and response shapes
are checked at compile time. `agent_id` names an installed agent; `coach` is
the one [Your first agent](/tutorials/your-first-agent) builds.

### Python

```python
import pathlib
from apollia_runtime_client import AuthenticatedClient
from apollia_runtime_client.api.tasks import submit_task, get_task
from apollia_runtime_client.models import SubmitTaskRequest, SubmitTaskRequestInput

token = (pathlib.Path.home() / ".apollia" / "api-token").read_text().strip()
client = AuthenticatedClient(base_url="http://127.0.0.1:7771", token=token)

resp = submit_task.sync(client=client, body=SubmitTaskRequest(
    agent_id="coach",
    input_=SubmitTaskRequestInput.from_dict(
        {"parts": [{"type": "text", "text": "hello"}]}
    ),
))
task = get_task.sync(id=resp.task_id, client=client)
```

## Streaming results

Beyond submit-and-poll, the contract exposes server-sent event streams for
long-running work: task output at `GET /api/v1/tasks/{id}/stream` and chat
session output at `GET /api/v1/sessions/{id}/stream`. `GET /api/v1/events`
streams the event bus of the runtime as a whole, for a host that follows every
run rather than one. Consult the
[HTTP API reference](/reference/api/apollia-os-runtime-api) for their event
shapes.

## Regenerating the clients

When the runtime contract changes, refresh the clients from the spec:

```sh
# From the committed spec:
bash clients/regen.sh

# Or refresh the spec from a running daemon first:
bash clients/regen.sh --from-daemon
```

## Related

- [Embed Apollia via federation (MCP + REST)](/how-to/embed-via-federation)
  for the sidecar integration pattern.
- [Audit and verify a run](/how-to/audit-and-verify) for the
  accountability workflow around what your integration runs.
- The [HTTP API reference](/reference/api/apollia-os-runtime-api),
  [CLI reference](/reference/cli), and [SDK reference](/reference/sdk).
