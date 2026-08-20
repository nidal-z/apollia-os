---
sidebar_position: 8
title: Deploy in production
---

# Deploy in production

This guide runs Apollia as a long-lived service on a Linux server: an optimized
build, a service manager, a hardened network posture, and the checks you run after
each deploy. It assumes you can already build and run the daemon locally; if not,
start with [Install and run the runtime](/how-to/install-and-run).

The `packaging/` directory in the repository builds desktop bundles (DMG and
AppImage) for end users, not a server daemon. For a server you build the binary
from source and wrap it in your init system, which is what this guide does.

## Build an optimized binary

```sh
cargo build -p apollia-cli --release
```

The result is `target/release/apollia-os`. Install it where your service will find
it:

```sh
sudo install -m 755 target/release/apollia-os /usr/local/bin/apollia-os
```

## Run under systemd

No unit file ships in the repository; you write one that wraps the real `start`
and `stop` commands. Run the daemon under a dedicated unprivileged user so an
agent cannot read the rest of the system.

```ini
# /etc/systemd/system/apollia.service
[Unit]
Description=Apollia OS runtime
After=network.target

[Service]
Type=simple
User=apollia
Group=apollia
ExecStart=/usr/local/bin/apollia-os start --port 7771
ExecStop=/usr/local/bin/apollia-os stop
Restart=on-failure
RestartSec=5s

[Install]
WantedBy=multi-user.target
```

```sh
sudo useradd --system --home /var/lib/apollia --shell /usr/sbin/nologin apollia
sudo systemctl daemon-reload
sudo systemctl enable --now apollia
sudo systemctl status apollia
```

## Harden the network posture

- **Default: loopback plus token.** The TCP API defaults to `127.0.0.1`, and TCP
  callers must present the bearer token written to `~/.apollia/api-token` on first
  boot (the Unix socket is local-trust). For a same-host integration, keep the bind
  on loopback and leave `[api].require_token = true`.
- **Expose over TLS, never in the clear.** To reach the daemon from another host,
  the runtime can terminate TLS itself: set `[api].tls_cert` and `[api].tls_key` in
  `apollia.toml` (both, or neither) and the TCP listener serves HTTPS directly, with
  no extra component to operate. Alternatively, keep the bind on loopback and put a
  reverse proxy that terminates TLS in front, forwarding to `127.0.0.1:7771`. Either
  way, keep the bearer token required.
- **Insecure binds fail fast.** Binding a non-loopback address with
  `require_token = false` is refused at startup, so a public port is never served
  unauthenticated by accident. Keep the token requirement on for any exposed
  interface.
- **Protect the token file.** `~/.apollia/api-token` for the service user grants
  full control of the runtime. Keep it readable only by that user.
- **Sandbox prerequisites (Linux), and they conflict with the unprivileged user
  above.** `bash_executor` and `python_executor` isolate their child process with
  `unshare --pid --mount --fork`. Those flags are called **without** `--user`, so
  they need `CAP_SYS_ADMIN`: enabling unprivileged user namespaces on the host
  does not grant it. Under a plain `User=apollia` service, both executors fail,
  and nothing runs outside the namespace. The refusal reaches the caller as the
  program's own non-zero exit rather than as a distinct sandbox error, so do not
  rely on it as a fail-closed signal.
  <!-- claim:unshare-sandbox-requires-cap-sys-admin -->

  Pick one, knowingly:

  - grant the capability to the unit, `AmbientCapabilities=CAP_SYS_ADMIN` plus
    `CapabilityBoundingSet=CAP_SYS_ADMIN`, and keep the unprivileged user;
  - or run without the two code-execution tools, disabling them in `[tools]`;
  - or accept that they will fail at call time.

  Running the service as root to get the capability trades a contained tool for
  an uncontained daemon, which is the wrong way round.

### Reverse proxy with TLS termination

If your infrastructure already terminates TLS upstream (Caddy, nginx, an ingress),
leave `[api].tls_cert` / `[api].tls_key` unset, keep the daemon on loopback, and
forward to `127.0.0.1:7771`.

```
apollia.example.com {
    reverse_proxy 127.0.0.1:7771
    # SSE: disable buffering for the streaming endpoints
    reverse_proxy /api/v1/tasks/*/stream 127.0.0.1:7771 {
        flush_interval -1
    }
    reverse_proxy /api/v1/mailbox/stream 127.0.0.1:7771 {
        flush_interval -1
    }
}
```

```nginx
server {
    listen 443 ssl;
    server_name apollia.example.com;
    ssl_certificate     /etc/ssl/apollia/fullchain.pem;
    ssl_certificate_key /etc/ssl/apollia/privkey.pem;

    location / {
        proxy_pass http://127.0.0.1:7771;
        proxy_set_header Authorization $http_authorization;
    }
    # SSE: unbuffered streaming
    location ~ ^/api/v1/(tasks/.*/stream|mailbox/stream)$ {
        proxy_pass http://127.0.0.1:7771;
        proxy_buffering off;
        proxy_cache off;
        proxy_read_timeout 3600s;
    }
}
```

The streaming endpoints `GET /api/v1/tasks/{id}/stream` and
`GET /api/v1/mailbox/stream` push events as they happen. Without buffering
disabled, the proxy holds them until the response closes and the host sees nothing
live. The same caution applies client-side under native TLS: do not buffer the
response.

## Verify after deploy

```sh
# Liveness
curl http://127.0.0.1:7771/api/v1/health          # {"status":"ok"}

# Runtime and agent status
apollia-os status

# End-to-end: install a trivial agent and run it
apollia-os agent install clients/examples/echo_agent.py --skip-tests
apollia-os run echo "post-deploy check"
```

The same records the CLI reads are available over HTTP for a host integration; see
the [HTTP API reference](/reference/api/apollia-os-runtime-api).

## Operate the running service

- **Logs.** Follow the runtime log with `apollia-os logs --follow`, or read the
  service journal with `journalctl -u apollia -f`.
- **Plan cache.** Orchestrated runs cache their plans. Inspect or clear it when
  diagnosing stale planning:

  ```sh
  apollia-os plan cache stats
  apollia-os plan cache clear
  ```

- **Audit.** Every governed action is recorded in a signed, hash-chained journal.
  Read and verify it with the [audit workflow](/how-to/audit-and-verify).

## Upgrade

```sh
git pull
cargo build -p apollia-cli --release
sudo systemctl stop apollia
sudo install -m 755 target/release/apollia-os /usr/local/bin/apollia-os
sudo systemctl start apollia
```

## Local inference on a server

To serve local GGUF models on the server, make `llama-server` (upstream llama.cpp)
available to the daemon: the daemon spawns and supervises it, and finds it on the
service user's `PATH`. Install it once where the service runs, exactly as described
in [Install and run the runtime](/how-to/install-and-run#local-gguf-inference).
Continuous batching and native tool calling are built into that engine, so a
single local backend already serves concurrent requests; see
[Get the most from local inference](/how-to/accelerate-local-inference).

## Related

- [Install and run the runtime](/how-to/install-and-run) for the build details.
- The [CLI reference](/reference/cli) for every operational command.
- [Audit and verify a run](/how-to/audit-and-verify) for the
  accountability workflow.
