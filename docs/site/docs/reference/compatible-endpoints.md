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

The server listens only once someone turns it on. Four keys of `apollia.toml`
hold its state:

```toml
[server]
enabled = false        # whether the compatible server listens
port = 7772            # its TCP port; never the native API's 7771
require_auth = false   # whether every request must present a token
cors = false           # whether a web page of another origin may call it
serve_on_lan = false   # whether the other devices of the local network reach it
```

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
| `GET /api/v1/server` | Whether it listens, its port, whether it serves the local network, the address to copy, why its last start failed, and how many tokens are in use |
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
| `404` | `model_not_loaded` | `not_found_error` | The model named in `model` is not loaded |
| `404` | `endpoint_not_served` | `not_found_error` | The engine running the model does not serve this endpoint |
| `404` | `unknown_url` | | No endpoint lives at this path |
| `502` | `engine_unavailable` | `api_error` | The engine running the model did not answer |

## The models it serves

The compatible server serves the local models loaded in Apollia, and only
them. A request names one in its `model` field, by the identifier
`GET /v1/models` lists, which is its identifier in Apollia's model registry.
The server passes the request to the engine that runs the model,
`llama-server` for a GGUF model, `vllm-mlx` for an MLX one, with `model` set to
what that engine expects, and passes the engine's answer back as it came: a
JSON body, or an event stream relayed chunk by chunk when the request asked for
`"stream": true`.

- A model that is not loaded answers `404 model_not_loaded`. Load it in Apollia
  first.
- A model of a remote provider is never relayed: the compatible server is not a
  gateway to a cloud.
- Nothing is translated from one format into another. An endpoint the engine of
  the model does not serve answers `404 endpoint_not_served` rather than an
  approximation. The search-by-meaning model serves `/v1/embeddings` and
  nothing else; a chat model serves every other endpoint below, except that
  `vllm-mlx` does not count tokens.

A model stays loaded for as long as a request to it runs, streams included:
unloading it waits until the answer ends.

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
