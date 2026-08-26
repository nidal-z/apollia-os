---
title: The sidecar runner does not start
slug: /operator-help/troubleshooting/the-runner-does-not-start
sidebar_position: 2
---

# The sidecar runner does not start

The **runner** (`apollia-runner-<backend>`) is the speech recognition sidecar (STT, whisper). The daemon spawns it at startup and talks to it over HTTP loopback. Local LLM inference does not go through the runner: it is served by the embedded `llama-server` engine. If it is the local LLM that does not answer, see [The AI provider does not answer](le-fournisseur-d-ia-ne-repond-pas.md) instead.

If you see messages such as:

- `RUNNER_HANDSHAKE_TIMEOUT`
- `runner sidecar not available (spawn failed)`

...the STT sidecar could not start and voice dictation is unavailable. Here are the common causes.

## 1. The runner binary is missing

The installation bundle must contain at least `apollia-runner-cpu` next to the daemon.

**macOS:** `~/Applications/Apollia\ OS.app/Contents/Resources/apollia-runner-*`
**Linux:** in the AppImage `.AppDir/usr/bin/` or next to `apollia-os` (`.deb` package)
**Windows:** `C:\Program Files\Apollia OS\apollia-runner-*.exe`

Check with:

```sh
apollia-os doctor
```

If `apollia-runner-cpu` is missing: reinstall Apollia (the bundle has been altered).

## 2. The GPU driver is missing or too old

The daemon detected your GPU and tried to spawn the matching STT runner (for example `apollia-runner-cuda`), but the runtime libraries are missing.

**Symptoms:**

- macOS: rarely blocking (Metal ships with the system).
- Linux/Windows CUDA: `libcuda.so.1 not found` or `nvcuda.dll not found`.
- Linux ROCm: `libhip.so not found`.
- Vulkan: `libvulkan.so.1 not found`.

**Fix:** update the GPU driver, or fall back to the CPU runner by copying `apollia-runner-cpu` next to the `apollia-os` binary (see the installation page for your system). Then restart: `apollia-os stop && apollia-os start`.

## 3. Firewall blocks the loopback connection (Windows)

The runner listens on `127.0.0.1:<auto-port>`. If Windows Defender Firewall blocked `apollia-runner-cuda.exe` on first launch, the daemon cannot connect to it.

**Fix:** `Settings > Privacy & security > Windows Security > Firewall & network protection > Allow an app`, then tick Apollia OS for private networks.

## 4. Slow cold start (Apple Silicon)

The first spawn of the Metal runner can take 5 to 15 seconds (MTLDevice init). If the handshake timeout fires: try again. If it happens repeatedly, check that `xcode-select -p` returns a valid path.

## 5. Collecting logs

```sh
apollia-os stop
APOLLIA_LOG=debug apollia-os start 2>&1 | tee /tmp/apollia.log
```

Look for:

- `supervisor.runner.spawned` (the daemon started it)
- `supervisor.runner.spawn.failed` (it did not start, and the daemon carries on
  without local STT)
- `runner.spawn.failed` (the binary could not be launched at all)
- `runner handshake timeout after 10s` (it started but never announced its port)
- `runner.respawned` / `runner.respawn.failed` (it died and the daemon retried)

The runner's own output is forwarded under the `runner` log target, so its
stderr appears in the same file.

Open a GitHub issue with `apollia.log` + the output of `apollia-os doctor --json`.
