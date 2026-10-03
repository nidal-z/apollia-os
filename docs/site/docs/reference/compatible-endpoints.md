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

The server listens only once someone turns it on. Three keys of
`apollia.toml` hold its state:

```toml
[server]
enabled = false   # whether the compatible server listens
port = 7772       # its TCP port, on 127.0.0.1; never the native API's 7771
cors = false      # whether a web page of another origin may call it
```

It listens on the loopback interface, `127.0.0.1`, so only the applications of
this machine reach it.

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

Three routes of the native API drive it while the runtime runs:

| Route | Effect |
|---|---|
| `GET /api/v1/server` | Whether it listens, its port, and why its last start failed |
| `POST /api/v1/server/start` | Opens it on the configured port and writes `enabled = true` |
| `POST /api/v1/server/stop` | Closes it and writes `enabled = false`; the native API keeps answering |

Because starting and stopping write `enabled`, the state survives a restart: a
runtime that starts with `enabled = true` opens the compatible server again. If
another program already holds the port, the start is refused with a readable
"port in use" error (HTTP 409 on the start route), and the runtime goes on
without the compatible server.

## Errors

Every error follows the OpenAI envelope, whatever the endpoint:

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
