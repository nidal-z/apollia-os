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
- **Sandbox prerequisites (Linux).** The `bash_executor` tool isolates commands
  with Linux namespaces (`unshare --pid --mount`). If your distribution restricts
  unprivileged user namespaces, enable them on the host, otherwise sandboxed shell
  execution fails.

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
  Read and verify it with the [audit workflow](/how-to/audit-verify-rollback).

## Upgrade

```sh
git pull
cargo build -p apollia-cli --release
sudo systemctl stop apollia
sudo install -m 755 target/release/apollia-os /usr/local/bin/apollia-os
sudo systemctl start apollia
```

## Local inference on a server

The release binary is cloud-only. To serve local GGUF models on the server, also
build the `apollia-runner` sidecar with your hardware backend and co-locate it
next to the installed `apollia-os` binary, exactly as described in
[Install and run the runtime](/how-to/install-and-run#optional-enable-local-gguf-inference).

## Related

- [Install and run the runtime](/how-to/install-and-run) for the build details.
- The [CLI reference](/reference/cli) for every operational command.
- [Audit, verify and roll back a run](/how-to/audit-verify-rollback) for the
  accountability workflow.
