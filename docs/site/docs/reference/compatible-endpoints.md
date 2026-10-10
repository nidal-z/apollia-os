---
sidebar_position: 10
title: Compatible endpoints
description: "The compatible server: a second listener of the runtime, on its own port, that other applications call in the OpenAI and Anthropic formats, for the loaded models and the agents and workflows made available. Off by default."
---

# Compatible endpoints

The runtime can open a second listener, the **compatible server**, for the
other applications of the machine. They call it the way they would call OpenAI
or Anthropic: the paths start with `/v1/`, the bodies follow those APIs and
their field names (`snake_case`), and an error comes back in the envelope of
the API the client called.

The compatible server is not the native API. The native API, documented in the
[HTTP API reference](/reference/api/apollia-os-runtime-api), stays on port 7771
and the local socket, under `/api/v1/`; the desktop application and the CLI
drive the runtime through it. The compatible server has its own port and serves
the endpoints listed below and nothing else, so starting or stopping it never
touches the runtime or the desktop.

## Off by default

The server listens only once someone turns it on. Five keys of `apollia.toml`
hold its state:

```toml
[server]
enabled = false        # whether the compatible server listens
port = 7772            # its TCP port; never the native API's 7771
require_auth = false   # whether every request must present a token
cors = false           # whether a web page of another origin may call it
serve_on_lan = false   # whether the other devices of the local network reach it
```

Four more say how it loads and unloads models on demand; see
[Loading on demand](#loading-on-demand).

By default it listens on the loopback interface, `127.0.0.1`, so only the
applications of this machine reach it.

## Calls from a web page (CORS)

A browser lets a page call a server of another origin only if that server
allows it, and it asks first: before a request carrying a bearer or a JSON body,
it sends an `OPTIONS` preflight. With `cors = false`, the default, the
compatible server sends no cross-origin header at all, and the browser refuses
the call. With `cors = true`, a page of any origin may call it:

- a preflight is answered `204 No Content` with
  `Access-Control-Allow-Origin: *`,
  `Access-Control-Allow-Methods: GET, POST, OPTIONS` and
  `Access-Control-Allow-Headers: authorization, content-type, x-api-key, anthropic-version`,
  the headers the OpenAI and Anthropic SDKs send;
- every other answer, an error included, carries
  `Access-Control-Allow-Origin: *`.

The preflight needs no token, since a browser never sends one on it; the
request that follows is checked like any other. The setting is read when the
server starts: after changing it, stop and start the server. The native API on
port 7771 never carries these headers, whatever the setting.

## Starting and stopping it

Four routes of the native API drive it while the runtime runs:

| Route | Effect |
|---|---|
| `GET /api/v1/server` | Whether it listens, its port, whether it serves the local network, the address to copy, why its last start failed, how many tokens are in use, and which of its settings the [organisation policy](#organisation-policy) fixes |
| `POST /api/v1/server/start` | Opens it on the configured port and writes `enabled = true` |
| `POST /api/v1/server/stop` | Closes it and writes `enabled = false`; the native API keeps answering |
| `POST /api/v1/server/restart` | Closes and reopens it so it applies the `[server]` keys as they stand; only this listener is reopened, the native API keeps its connections. A stopped server stays stopped |

Because starting and stopping write `enabled`, the state survives a restart: a
runtime that starts with `enabled = true` opens the compatible server again. If
another program already holds the port, the start is refused with a readable
"port in use" error (HTTP 409 on the start route), and the runtime goes on
without the compatible server.

## Tokens

With `require_auth = true`, every request must present a token of the server.
Each application gets its own, named after it, so one can be withdrawn without
touching the others. The token the desktop uses to reach its runtime is not one
of them and never opens the compatible server; a server token never opens the
native API.

| Route | Effect |
|---|---|
| `POST /api/v1/server/tokens` | Creates a token from its `name` and answers its value, `apo_` followed by 64 hexadecimal characters. The value is shown this once and kept nowhere |
| `GET /api/v1/server/tokens` | Lists every token: name, last four characters, creation, last use, revocation. Never the value |
| `DELETE /api/v1/server/tokens/{token}` | Revokes a token by identifier or by name; it stays listed, marked revoked |

Only the SHA-256 of a value is stored, in `system.db`. A client sends its token
the way its own API expects, either header works:

```text
Authorization: Bearer apo_...   # OpenAI clients
x-api-key: apo_...              # Anthropic clients
```

A request without a token in use, or with a revoked one, answers `401` in the
format of the endpoint it called. Each success stamps the token's last use. The
setting is read when the server opens: restart it to apply a change.

## Local network

With `serve_on_lan = true`, the compatible server listens on every interface,
so the other devices of the local network call it at the machine's address.
The `address` field of `GET /api/v1/server` gives the address to copy:
`http://127.0.0.1:7772` when the server stays on the machine,
`http://<the machine's IPv4 address on the network>:7772` when it serves the
network. The key is read when the server opens: restart it to apply a change.

The local network and the tokens are two separate switches:

- **Models** answer the network with or without a token, as `require_auth`
  says. Opening the network without tokens is allowed, as other local
  inference servers allow it; anyone on the network can then use the models.
- **Agents and workflows** never answer a call from the network without a token
  in use, whatever `require_auth` says: they reach the user's connections and
  documents. The refusal is a `401` that reads "A token is required to call an
  agent or a workflow from the local network."
- **The native API** stays on the machine in every case: port 7771 and the
  local socket never listen on the network because of this key.

Each request is classified, when its connection is accepted, as coming from the
machine (the loopback interface) or from the local network (any other address).
The decision and its reading against the local-first principle are written in the
[architecture decisions](/architecture/decisions#local-server).

## Organisation policy

An organisation that deposits a signed policy on the machine closes three
settings of the compatible server, through the `[settings]` table of its
`policy.toml`:

```toml
[settings]
"server.serve_on_lan" = false        # the server stays on the machine
"server.require_auth" = true         # every request presents a token
"server.allow_request_mcp" = false   # no request brings an MCP server
```

| Key | The value a policy fixes | What it closes |
|---|---|---|
| `server.serve_on_lan` | `false` | The local network: the server listens on the loopback interface alone |
| `server.require_auth` | `true` | A server without a token: every request presents one |
| `server.allow_request_mcp` | `false` | The MCP servers a request brings: such a request answers `400`, and the message names the policy |

A policy only closes. One that fixes any of the three at the other value is
refused whole when it is read, like a policy that names an unknown key: opening
the server stays an action of the user, never of a file dropped on the machine.

A key the policy fixes is read from the policy, whatever `apollia.toml` says,
and writing it is refused with "setting 'server.serve_on_lan' is locked by the
organization policy". `GET /api/v1/server` says which keys are fixed in its
`imposedByPolicy` field, one boolean per key: `serveOnLan`, `requireAuth`,
`allowRequestMcp`.

The runtime reads the policy when it starts, before the server opens. A server
that `apollia.toml` had left on and open against the policy therefore opens
closed, and the audit journal keeps one entry per key closed that way, of the
kind `policy_setting_enforced`, in the run `governance:policy:organisation`. The
entry names the key and holds no value. How a policy is deposited and signed is
written in the [architecture decisions](/architecture/decisions#organisation-policy).

## Logs

The runtime keeps the most recent lines of the server in memory, 2000 at most,
the oldest dropped first: one per request it answered (time, level, method,
path, model, status, duration) and each line the local `llama-server` prints.
A refused request (4xx) is at the `warn` level, a failed one (5xx) at `error`.

| Route | Effect |
|---|---|
| `GET /api/v1/server/logs?level=warn` | An event stream: one `log` frame per line, those held first, then each new one. `level` is the lowest level sent, `debug` (the default), `info`, `warn` or `error` |
| `DELETE /api/v1/server/logs` | Empties the lines held and answers how many there were. The audit journal is not touched |

These lines are a view for whoever develops against the server, not a record:
they are lost when the runtime stops.

## Audit journal

The record is the audit journal. Every request the server receives leaves one
entry there, of the kind `server_call`, in the run `server:calls`, chained and
signed like every other entry: a request answered, one refused (`401`, `400`)
and one that failed alike.

| Field | What it holds |
|---|---|
| `method`, `endpoint` | The method and the path asked for, without its query string |
| `target`, `target_kind` | The model named, or the agent or workflow (`model` or `agent`); absent when the request named none the server could read |
| `token_name` | The name of the token presented, never its value; absent without a token in use |
| `caller_address`, `origin` | The address the connection was accepted from, and `loopback` or `local_network` |
| `status` | The status answered |
| `prompt_tokens`, `completion_tokens` | The tokens the answer stated, read in passing from a stream; absent when it stated none |
| `duration_ms` | How long the answer took; for a stream, until it closed |
| `task_id`, `run_id` | For a call to an agent or a workflow, the task and the run it started |

An entry never holds a message, an answer, a header or a token value. A header
that claims another address is not read: the address is the one of the
connection. The entries of an agent run are kept under the `run_id` its call
names, so what an application asked an agent to do is followed from the call.

## Errors

Each endpoint answers its errors in the format of its own API. The OpenAI
endpoints, and any path the server does not serve, use the OpenAI envelope:

```json
{
  "error": {
    "message": "Unknown endpoint: /v1/unknown",
    "type": "invalid_request_error",
    "code": "unknown_url"
  }
}
```

The Anthropic endpoints (`/v1/messages` and `/v1/messages/count_tokens`) use
Anthropic's: `{"type": "error", "error": {"type", "message"}}`.

| Status | OpenAI `code` | Anthropic `type` | When |
|---|---|---|---|
| `400` | `invalid_request` | `invalid_request_error` | The body is not a JSON object, or names no `model` |
| `401` | `invalid_api_key` | `authentication_error` | `require_auth` is on and no token in use was presented |
| `404` | `model_not_loaded` | `not_found_error` | The model named in `model` is not loaded, and is not loaded on demand |
| `404` | `endpoint_not_served` | `not_found_error` | The engine running the model does not serve this endpoint |
| `404` | `unknown_url` | | No endpoint lives at this path |
| `502` | `engine_unavailable` | `api_error` | The engine running the model did not answer, or the model could not be loaded on demand |

An agent or a workflow called as a model adds its own errors, on
`/v1/chat/completions`:

| Status | OpenAI `code` | When |
|---|---|---|
| `400` | `invalid_request` | The request brings `tools`, holds no user message, or, for a workflow, its last user message is not a JSON object that fits the input schema; the error then carries that schema in `schema` |
| `401` | `invalid_api_key` | The call comes from the local network without a token in use |
| `404` | `model_not_found` | No agent or workflow available to other applications carries this `apollia/...` identifier |
| `409` | `approval_pending` | The agent waits for a decision on an access nobody granted (see below) |
| `500` | `run_failed` | The run failed or was stopped before its end |
| `503` | `run_unavailable` | This runtime cannot run agents or workflows |

## The models it serves

The compatible server serves the local models of this machine, and only them.
A request names one in its `model` field, by the identifier
`GET /v1/models` lists, which is its identifier in Apollia's model registry.
The server passes the request to the engine that runs the model,
`llama-server` for a GGUF model, `vllm-mlx` for an MLX one, with `model` set to
what that engine expects, and passes the engine's answer back as it came: a
JSON body, or an event stream relayed chunk by chunk when the request asked for
`"stream": true`.

- A language model of the registry that is not loaded is loaded for the
  request, unless loading on demand is off; then it answers
  `404 model_not_loaded`, and has to be loaded in Apollia first.
- A model of a remote provider is never relayed: the compatible server is not a
  gateway to a cloud.
- Nothing is translated from one format into another. An endpoint the engine of
  the model does not serve answers `404 endpoint_not_served` rather than an
  approximation. The search-by-meaning model serves `/v1/embeddings` and
  nothing else; a chat model serves every other endpoint below, except that
  `vllm-mlx` does not count tokens.

A model stays loaded for as long as a request to it runs, streams included:
unloading it waits until the answer ends.

## Loading on demand

The server loads the model a request names when it is not loaded yet, and
gives the memory of the models it loaded back on its own.

```toml
[server]
jit_loading = true            # a request loads the model of this machine it names
jit_auto_unload = true        # the models the server loaded have their own idle delay
jit_idle_ttl_minutes = 60     # that delay; 0 keeps them loaded
jit_keep_last_only = true     # loading one unloads the others the server loaded
```

| Key | Default | Effect |
|---|---|---|
| `jit_loading` | `true` | A request naming a language model of the registry that is not loaded loads it first. Off, it answers `404 model_not_loaded`. With it on, `GET /v1/models` lists the language models of this machine as well as the loaded ones |
| `jit_auto_unload` | `true` | A model the server loaded is unloaded after `jit_idle_ttl_minutes` without a request, instead of the general delay of `[llm.residency]`. Off, it follows the general delay |
| `jit_idle_ttl_minutes` | `60` | The delay above, in minutes; `0` keeps those models loaded |
| `jit_keep_last_only` | `true` | When the server loads a model, it unloads the other models it loaded |

The list of loaded models of the native API, `GET /api/v1/llm/loaded-models`,
tells them apart: a model the server loaded carries `"origin": "server"`, the
others `"origin": "app"`. The origin is fixed when the model loads: a model
the application loaded stays the application's when a request uses it.

None of these rules unloads a model the user pinned, a model an open
conversation uses, or a model answering a request, and keeping only the last
model never touches a model the application loaded. Each unload is published as
`LlmModelUnloaded`, with the reason `idle`, or `server_replaced` when the
server loaded another model. The keys are read at every request and every
check, so a change applies without a restart.

## Reasoning of a thinking model

A thinking model writes its reasoning before its answer. The engine separates
the two, and a chat completion carries the reasoning in `reasoning_content`, in
the message of a whole answer and in each delta of a stream, apart from
`content`. That is what the server relays by default.

```toml
[server]
separate_reasoning_content = true   # false: the reasoning goes back into content
```

With `separate_reasoning_content = false`, the server puts the reasoning back at
the head of `content`, between `<think>` and `</think>`, and drops
`reasoning_content`, for a client that reads only `content`. In a stream, the
tag opens on the first delta of reasoning and closes on the first delta of the
answer, or on the delta that finishes the choice. Only `/v1/chat/completions`
is concerned: the responses and messages formats carry reasoning blocks of
their own, relayed as the engine wrote them. The key is read at every request.

## Agents and workflows as models

An agent or a workflow the user made available to other applications, the
same switch the MCP bridge reads, is served as a model too. `GET /v1/models`
lists it after the local models, as `apollia/<id>` with the `type` `agent` or
`workflow`: an agent by its identifier (`apollia/demand-clerk`), a workflow by
its package and its name (`apollia/office/close-month`). An agent identifier
never carries `/`, so an agent and a workflow never answer to the same name.

A client calls it as it would call a model, with the client it already uses:
`POST /v1/chat/completions`, `POST /v1/responses` or `POST /v1/messages`. The
rules below hold for the three; the example and the field names are those of
chat completions, and [the two other formats](#responses-and-messages) say
where each puts the same things.

```bash
curl http://127.0.0.1:7772/v1/chat/completions \
  -H "content-type: application/json" \
  -d '{
    "model": "apollia/demand-clerk",
    "messages": [{"role": "user", "content": "Qualify the demand of Durand."}]
  }'
```

- **The call holds no state.** For an agent, the last user message is the
  request; the messages before it are the conversation it comes from. For a
  workflow, the last user message is a JSON object that fits the workflow's
  input schema (`{"month": "2026-09"}`); anything else answers `400` with the
  schema.
- **The agent works as in Apollia.** It runs with nobody watching, with its
  own budget, accesses and connections, through the same execution as a batch
  item. Tools sent by the client are refused with a `400`: the agent has its
  accesses.
- **The answer is the agent's final answer**, in `choices[0].message.content`.
  `usage` adds up the tokens of every model turn of the run.
- **The steps are reasoning.** Each tool the agent calls, and the summary of
  what it returned, is a line of `reasoning_content`, the field local inference
  servers use for a model's reasoning: in the message of a whole answer, and,
  with `"stream": true`, in chunks that come before the answer's `content`
  chunk. A client that does not read the field ignores it.
- **A client that goes away stops the run.** Closing the connection before the
  answer, or leaving a stream, stops the agent or the workflow.
- **The local network needs a token.** A call to an agent or a workflow from
  another device presents a token in use, whatever `require_auth` says, and
  `GET /v1/models` does not list them to a device without one.

### Responses and messages

The same run answers a client of the OpenAI Responses API and a client of the
Anthropic Messages API. Only the reading of the request and the writing of the
answer change:

| | `POST /v1/responses` | `POST /v1/messages` |
|---|---|---|
| The request | The last item of `input` whose role is `user`, or `input` itself when it is a text | The last message of `messages` whose role is `user` |
| The conversation | The message items before it; an item that is not a message is left out | The messages before it |
| Instructions of the client | `instructions` | `system`, a text or text blocks |
| The answer | The last item of `output`, a `message` holding one `output_text` | The last block of `content`, a `text` block |
| The steps | One `reasoning` item per step, before the message, its text in `summary` | One `thinking` block per step, before the text |
| `usage` | `input_tokens`, `output_tokens`, `total_tokens` | `input_tokens`, `output_tokens` |

```bash
curl http://127.0.0.1:7772/v1/messages \
  -H "content-type: application/json" \
  -H "anthropic-version: 2023-06-01" \
  -d '{
    "model": "apollia/demand-clerk",
    "max_tokens": 1024,
    "system": "Answer in one line.",
    "messages": [{"role": "user", "content": "Qualify the demand of Durand."}]
  }'
```

- **Instructions are context, never the mission.** `system` and `instructions`
  open the conversation the agent reads, as a `system` message does in a chat
  completion. They do not replace the mission the agent was given in Apollia.
- **A response holds no state either.** `previous_response_id` and
  `conversation` are refused with a readable `400`: send the earlier messages
  in `input`. `store` is accepted and ignored, nothing is kept to read back.
- **A stream follows the events of its API.** For a response:
  `response.created`, then for each step a reasoning item
  (`response.output_item.added`, `response.reasoning_summary_text.delta`,
  `response.output_item.done`), then the message item with the answer in
  `response.output_text.delta`, then `response.completed` carrying the usage.
  For a message: `message_start`, then for each step a `thinking` block
  (`content_block_start`, a `thinking_delta`, `content_block_stop`), then the
  `text` block with the answer in a `text_delta`, then `message_delta` carrying
  `stop_reason` and the usage, and `message_stop`.
- **A `thinking` block has an empty `signature`.** The steps are Apollia's
  account of the work, not the signed reasoning of a model.
- **Tokens are not counted beforehand.** `/v1/messages/count_tokens` serves
  local models only; on an `apollia/...` model it answers `404`.

### A decision the agent waits for

An agent that reaches for an access nobody granted, such as writing a file
without "always" answered for it, does not run it and does not fail. Its run
stops and waits for a decision, as an item of a batch does: it appears in the
pending decisions of Apollia, with its notification, and the call answers
`409` with the code `approval_pending`:

```json
{
  "error": {
    "message": "The agent waits for your approval to use an access nobody granted. ...",
    "type": "approval_pending",
    "code": "approval_pending",
    "pending_id": "batch:3f1c...:0",
    "batch_id": "3f1c...",
    "task_id": "9a7e...",
    "access": "file_write"
  }
}
```

On `/v1/messages`, the same fields sit in the error object of the Anthropic
envelope, whose `type` is `approval_pending`:

```json
{
  "type": "error",
  "error": {
    "type": "approval_pending",
    "message": "The agent waits for your approval to use an access nobody granted. ...",
    "pending_id": "batch:3f1c...:0",
    "batch_id": "3f1c...",
    "task_id": "9a7e...",
    "access": "file_write"
  }
}
```

In a stream, the same error object is the last event: as it stands for a chat
completion, as an `error` event for a message, and for a response as an `error`
event whose fields (`code`, `message`, `pending_id` and the others) sit beside
its `type`. Ending the answer does
not withdraw the decision: it stays pending until someone answers it, in
Apollia or through the native API with
`POST /api/v1/pending-decisions/decide` and the `pending_id`. Once allowed, the
run goes on; its result is read from the task (`GET /api/v1/tasks/{task_id}`)
or the batch (`GET /api/v1/batches/{batch_id}`).

## MCP servers a request brings

A response or a message can bring an MCP server of its own, to try a tool with
a model without adding the server as a connection. The compatible server reads
the two public forms: the `mcp` tool of the Responses API and the
`mcp_servers` of the Messages API.

```toml
[server]
allow_request_mcp = false   # whether a request may bring an MCP server
```

The setting is off by default, and read at every request. While it is off, a
request that brings a server answers `400`: "MCP servers brought by a request
are turned off."

```bash
curl http://127.0.0.1:7772/v1/responses \
  -H "content-type: application/json" \
  -d '{
    "model": "acme/chat",
    "input": "List the open tickets.",
    "tools": [{
      "type": "mcp",
      "server_label": "tickets",
      "server_url": "http://localhost:3000/mcp",
      "authorization": "the-token-of-that-server"
    }]
  }'
```

| | `POST /v1/responses` | `POST /v1/messages` |
|---|---|---|
| Where the server is named | An entry of `tools` whose `type` is `mcp` | An entry of `mcp_servers` whose `type` is `url` |
| Its address | `server_url` | `url` |
| Its secret | `authorization`, sent as a bearer; `headers`, an object of strings | `authorization_token`, sent as a bearer |
| The tools kept | `allowed_tools`, a list of names or `{"tool_names": [...]}` | `tool_configuration.allowed_tools`; `tool_configuration.enabled: false` leaves the server out |
| Its label | `server_label`, read and never used as a name | `name`, read and never used as a name |

With the setting on, such a request is not relayed to the engine of the model.
Apollia connects to the server, offers its tools to the model, runs each tool
the model calls and answers once the model has its final answer, in the format
of the endpoint: the answer and the steps sit where an
[agent called as a model](#responses-and-messages) puts them. What bounds it:

- **A model only.** On an `apollia/...` model the request answers `400`: an
  agent or a workflow works with the connections it was given in Apollia.
- **HTTP only.** The server is reached over streamable HTTP, at an `http` or
  `https` address. A request never starts a process: a `stdio` server, a
  `command`, or an address of another scheme answers `400`. A `connector_id`
  is not served.
- **This machine or a public host.** The address is `localhost`, a loopback
  address, or a public host. A server on the private network, a link-local
  address or an internal domain answers `400`, and so does a redirect towards
  one: a public server cannot send the connection to an internal address, and
  a server on this machine cannot redirect at all.
- **The organisation policy admits it.** Each server passes the admission a
  connection passes when someone adds it. With `[connections] remote = "deny"`
  in the policy, a server that is not on this machine answers `403` with the
  reason of the policy, before anything is connected.
- **The connection lasts as long as the call.** It is opened for the request
  and removed when the answer ends, and when the client goes away before the
  end. Nothing is written to the connections of Apollia.
- **Its tools are unclassified.** Whatever the server declares about a tool,
  Apollia treats it as sensitive, and nobody watches the call to approve it.
  The first tool the model calls ends the call on `409 approval_pending`,
  naming the tool in `access`, unless a rule set in the developer mode of
  Apollia allows that tool. Nothing waits for a decision afterwards, since the
  connection is gone: set the rule, then send the request again.
- **No other tool.** The tool calls are run by Apollia, so the request cannot
  bring function tools beside the server; it answers `400`.
- **One server per host, four at most.** A server is named after the host of
  its address, `request-mcp:<host>`, and its tools
  `mcp:request-mcp:<host>/<tool>`: that is the name a rule and the journal
  read. Two servers on one host in a request answer `400`.
- **The local network needs a token**, as a call to an agent does, whatever
  `require_auth` says.
- **The sampling fields are not applied.** The run uses the settings the model
  has in Apollia; `temperature`, `max_tokens` and the like are read and left
  aside. Instructions (`instructions`, `system`) open the conversation.

| Status | OpenAI `code` | Anthropic `type` | When |
|---|---|---|---|
| `400` | `invalid_request` | `invalid_request_error` | The setting is off, the model is an agent or a workflow, the server asks for a process, its address is not allowed, or the request brings other tools |
| `401` | `invalid_api_key` | `authentication_error` | The call comes from the local network without a token in use |
| `403` | `refused_by_policy` | `permission_error` | The organisation policy refuses the server |
| `409` | `approval_pending` | `approval_pending` | The model called a tool no rule allows; `access` names it |
| `502` | `mcp_server_unavailable` | `api_error` | The server did not answer the handshake |
| `503` | `run_unavailable` | `api_error` | This runtime cannot run the request, the model is not one Apollia runs agents on yet, or another request is using a server on the same host |

## Endpoints

The table lists every path the compatible server serves.

<!-- compat-endpoints -->

| Method | Path | Format |
|---|---|---|
| `GET` | `/v1/models` | OpenAI. The loaded models, as a list: `id` (registry identifier), `object`, `created`, `owned_by`, and `type`, `llm` or `embeddings`; then the agents and workflows available to other applications, `apollia/<id>`, of `type` `agent` or `workflow` |
| `POST` | `/v1/chat/completions` | OpenAI. Relayed to the model's engine; on an `apollia/...` model, run by Apollia (agents and workflows above) |
| `POST` | `/v1/responses` | OpenAI. Relayed to the model's engine; on an `apollia/...` model, or with an `mcp` tool, run by Apollia |
| `POST` | `/v1/embeddings` | OpenAI. Relayed to the search-by-meaning model |
| `POST` | `/v1/messages` | Anthropic. Relayed to the model's engine; on an `apollia/...` model, or with `mcp_servers`, run by Apollia |
| `POST` | `/v1/messages/count_tokens` | Anthropic. Relayed to the model's engine (`llama-server` only) |

Measured on `llama-server` at the release Apollia pins (`b11541`): the four
chat endpoints answer `200` on a chat model, and `/v1/embeddings` answers only
on the instance launched for search by meaning.

### Not the agent mailbox

`/v1/messages` here is Anthropic's Messages API: a conversation turn with a
model. It has nothing to do with `/api/v1/agents/{name}/messages` of the native
API, which is the mailbox through which agents send each other messages. The
two share a word and nothing else.
