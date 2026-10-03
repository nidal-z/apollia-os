---
sidebar_position: 10
title: Compatible endpoints
description: "The compatible server: a second listener of the runtime, on its own port, that other applications call in the OpenAI and Anthropic formats. Off by default."
---

# Compatible endpoints

The runtime can open a second listener, the **compatible server**, for the
other applications of the machine. They call it the way they would call OpenAI
or Anthropic: the paths start with `/v1/`, the bodies follow those APIs and
their field names (`snake_case`), and an error comes back in the OpenAI
envelope.

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
serve_on_lan = false   # whether the other devices of the local network reach it
```

By default it listens on the loopback interface, `127.0.0.1`, so only the
applications of this machine reach it.

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

## Errors

Every error follows the OpenAI envelope, except a `401` on the Anthropic
endpoint (`/v1/messages`), which follows Anthropic's
(`{"type": "error", "error": {"type": "authentication_error", "message"}}`):

```json
{
  "error": {
    "message": "Unknown endpoint: /v1/unknown",
    "type": "invalid_request_error",
    "code": "unknown_url"
  }
}
```

A path the server does not serve answers `404` with that body.

## Endpoints

The table lists every path the compatible server serves. It is empty today:
the listener is in place and serves no endpoint yet, so every path answers the
`404` above.

<!-- compat-endpoints -->

| Method | Path | Format |
|---|---|---|
