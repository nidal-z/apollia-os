---
sidebar_position: 8
title: Deploy in production
---

# Deploy in production

This guide runs Apollia as a long-lived service on a Linux server: an optimized
build, a service manager, a hardened network posture, and the checks you run after
each deploy. It assumes you can already build and run the daemon locally; if not,
start with [Install and run the runtime](/how-to/install-and-run).

The `packaging/` directory in the repository stages what a desktop bundle needs,
the embedded Python bundle and the `llama-server` binary, and the desktop bundle
itself is assembled by Tauri through
`crates/apollia-desktop/scripts/bundle-cli.sh`. Neither produces a server daemon.
For a server you build the binary from source and wrap it in your init system,
which is what this guide does.

## Build an optimized binary

```sh
bash packaging/fetch-tesseract-sources.sh
cargo build -p apollia-cli --release --features ocr
```

The `ocr` feature, off by default and on in the shipped archives, compiles the
embedded Tesseract engine that reads scanned pages and pictures in sources.
The script stages the Leptonica and Tesseract sources that build compiles,
checked against `packaging/tesseract-sources-checksums.txt`, so it downloads
nothing unverified. Both steps need `cmake`, a C and C++ compiler, `curl`,
`shasum` or `sha256sum`, and `unzip` or `python3`; on Debian or Ubuntu,
`sudo apt-get install -y cmake build-essential curl unzip`. A build machine
without a network sets `TESSERACT_SOURCES_DIR` to a directory holding
`leptonica-1.87.0.zip` and `tesseract-5.5.2.zip`, verified the same way.
Without `--features ocr` the binary builds without those tools but does not
read scanned pages.

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
sudo useradd --system --create-home --home /var/lib/apollia --shell /usr/sbin/nologin apollia
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
    reverse_proxy /api/v1/sessions/*/stream 127.0.0.1:7771 {
        flush_interval -1
    }
    reverse_proxy /api/v1/events 127.0.0.1:7771 {
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
    location ~ ^/api/v1/(tasks/.*/stream|sessions/.*/stream|mailbox/stream|events)$ {
        proxy_pass http://127.0.0.1:7771;
        proxy_buffering off;
        proxy_cache off;
        proxy_read_timeout 3600s;
    }
}
```

The streaming endpoints `GET /api/v1/tasks/{id}/stream`,
`GET /api/v1/sessions/{id}/stream`, `GET /api/v1/mailbox/stream` and
`GET /api/v1/events` push events as they happen. Without buffering
disabled, the proxy holds them until the response closes and the host sees nothing
live. The same caution applies client-side under native TLS: do not buffer the
response.

## Verify after deploy

```sh
# Liveness. The health route is the one API path served without the bearer
# token; it reveals the product marker and the version only.
curl -s http://127.0.0.1:7771/api/v1/health
# {"status":"ok","product":"apollia-os","version":"<runtime version>"}

# Runtime and agent status
apollia-os status

# End-to-end: build the starting package, install it and run its agent.
apollia-os package new postcheck
apollia-os package build postcheck
apollia-os package install postcheck-0.1.0.apo
apollia-os run postcheck "post-deploy check"
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
  apollia-os plan cache clear --confirm
  ```

- **Audit.** Every governed action is recorded in a signed, hash-chained journal.
  Read and verify it with the [audit workflow](/how-to/audit-and-verify).

## Upgrade

```sh
git pull
bash packaging/fetch-tesseract-sources.sh
cargo build -p apollia-cli --release --features ocr
sudo systemctl stop apollia
sudo install -m 755 target/release/apollia-os /usr/local/bin/apollia-os
sudo systemctl start apollia
```

## Local inference on a server

To serve local GGUF models on the server, the daemon spawns and supervises
`llama-server` (upstream llama.cpp). It takes the first binary it finds, in this
order:

1. the path in `APOLLIA_LLAMA_SERVER_BIN`, when the variable is set;
2. a build of the engine store, as the `engines.gguf` setting chooses: the
   variant it names, or, under `auto`, the installed build rated recommended
   for the machine, otherwise the best compatible one;
3. a `llama-server` next to the `apollia-os` binary, or in the `runners/`
   directory of a packaged layout around it;
4. the service user's `PATH`;
5. `/opt/homebrew/bin`, `/usr/local/bin`, `/usr/bin`, then `~/.local/bin` and
   `~/.cargo/bin` of the service user.

A source build ships no engine of its own, so on a fresh server the engine
store is the short way: [Enable CUDA on a server](/how-to/enable-cuda-on-a-server)
installs a build for the card with `apollia-os engine install`, online or from
a directory. A `llama-server` installed by hand, as described in
[Install and run the runtime](/how-to/install-and-run#local-gguf-inference),
is found on the `PATH` when the store holds nothing. Continuous batching and
native tool calling are built into that engine, so a single local backend
already serves concurrent requests; see
[Get the most from local inference](/how-to/accelerate-local-inference).

The daemon launches the engine without any `LLAMA_ARG_*` variable from its own
environment, so a variable left in the shell or the service unit cannot switch
an engine flag on; pass extra engine flags through `APOLLIA_LLAMA_EXTRA_ARGS`.

## Read scanned pages without a network

A source reads the text of a scanned page or a picture by optical character
recognition, which needs the language data of the embedded Tesseract engine:
one `<language>.traineddata` file per language. The daemon fetches the pinned
files the first time a pass leaves pages waiting for them. A server without a
network takes the same files from a disk. All of this needs a binary built
with `--features ocr`, as above; without it, the `recognition` check of
`doctor` reads `not compiled, <n>/2 languages verified: this build does not
read scanned pages`.

| File | Size in bytes | SHA-256 |
|---|---|---|
| `fra.traineddata` | 1130365 | `ced037562e8c80c13122dece28dd477d399af80911a28791a66a63ac1e3445ca` |
| `eng.traineddata` | 4113088 | `7d4322bd2a7749724879683fc3912cb542f19906c83bcc1a52132556427170b2` |

Both come from the upstream repository `tesseract-ocr/tessdata_fast` at commit
`923915d4ced2a7235221788285785a29c4a42d4a`. On a machine with a network, take
them from that commit and check them:

```sh
for lang in fra eng; do
  curl -fsSL -o "$lang.traineddata" \
    "https://raw.githubusercontent.com/tesseract-ocr/tessdata_fast/923915d4ced2a7235221788285785a29c4a42d4a/$lang.traineddata"
done
sha256sum fra.traineddata eng.traineddata
```

Copy them into the `tessdata/` directory of the models folder of the service
user: `<data directory>/models/tessdata/`, that is
`/var/lib/apollia/.apollia/models/tessdata/` for the service above, or
`<dir>/tessdata/` when `[models] dir` in `apollia.toml` names another models
folder. A file whose digest is not the pinned one is never handed to the
engine. The pages that waited are read again within one sweep of the sources,
without a restart.

Check it:

```sh
sudo -u apollia -H apollia-os doctor
sudo -u apollia -H apollia-os source show <id>
```

The `recognition` check of `doctor` reads `compiled, 2/2 languages verified`
once both files are in place, and warns, with the folder it read, while none
is. `source show` counts the pages of a source still `awaiting recognition`;
the line goes away once they are read.

## Related

- [Install and run the runtime](/how-to/install-and-run) for the build details.
- The [CLI reference](/reference/cli) for every operational command.
- [Audit and verify a run](/how-to/audit-and-verify) for the
  accountability workflow.
