---
sidebar_position: 10
title: Compatible endpoints
description: "The compatible server: a second listener of the runtime, on its own port, that other applications call in the OpenAI and Anthropic formats. Off by default."
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

The server listens only once someone turns it on. Three keys of `apollia.toml`
hold its state:

```toml
[server]
enabled = false        # whether the compatible server listens
port = 7772            # its TCP port, on 127.0.0.1; never the native API's 7771
require_auth = false   # whether every request must present a token
```

Four more say how it loads and unloads models on demand; see
[Loading on demand](#loading-on-demand).

It listens on the loopback interface, `127.0.0.1`, so only the applications of
this machine reach it.

## Starting and stopping it

Three routes of the native API drive it while the runtime runs:

| Route | Effect |
|---|---|
| `GET /api/v1/server` | Whether it listens, its port, why its last start failed, and how many tokens are in use |
| `POST /api/v1/server/start` | Opens it on the configured port and writes `enabled = true` |
| `POST /api/v1/server/stop` | Closes it and writes `enabled = false`; the native API keeps answering |

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
setting is read when the server opens: stop and start it to apply a change.

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

## Endpoints

The table lists every path the compatible server serves.

<!-- compat-endpoints -->

| Method | Path | Format |
|---|---|---|
| `GET` | `/v1/models` | OpenAI. The loaded models, as a list: `id` (registry identifier), `object`, `created`, `owned_by`, and `type`, `llm` or `embeddings` |
| `POST` | `/v1/chat/completions` | OpenAI. Relayed to the model's engine |
| `POST` | `/v1/responses` | OpenAI. Relayed to the model's engine |
| `POST` | `/v1/embeddings` | OpenAI. Relayed to the search-by-meaning model |
| `POST` | `/v1/messages` | Anthropic. Relayed to the model's engine |
| `POST` | `/v1/messages/count_tokens` | Anthropic. Relayed to the model's engine (`llama-server` only) |

Measured on `llama-server` at the release Apollia pins (`b10092`): the four
chat endpoints answer `200` on a chat model, and `/v1/embeddings` answers only
on the instance launched for search by meaning.

### Not the agent mailbox

`/v1/messages` here is Anthropic's Messages API: a conversation turn with a
model. It has nothing to do with `/api/v1/agents/{name}/messages` of the native
API, which is the mailbox through which agents send each other messages. The
two share a word and nothing else.
