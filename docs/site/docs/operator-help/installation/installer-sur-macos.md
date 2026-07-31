# Install Apollia on macOS

Apollia ships for macOS in two formats:

- **`.dmg`** (recommended): full desktop application, drag it into `Applications`.
- **`apollia-os-macos-silicon.tar.gz`**: CLI bundle only (power users).

## Requirements

- macOS 13 (Ventura) or newer.
- Apple Silicon (M1, M2, M3, M4). Intel support through Rosetta is not official for v0.1.0.
- 4 GB of free RAM minimum (8 GB recommended for Mistral-7B).
- 10 GB of disk space for the bundle + one quantized model.

## Installation (DMG)

1. Download `Apollia-OS_<version>.dmg` from the Releases page.
2. Double-click the file, drag `Apollia OS.app` into the `Applications` folder.
3. First launch: `Cmd+click` the icon then `Open` (Gatekeeper blocks unsigned apps).
4. The app starts the `apollia-os` daemon automatically. The daemon serves local LLM inference through the embedded `llama-server` engine and launches the speech-to-text (STT) runner.

## Verification

From a terminal:

```sh
~/Applications/Apollia\ OS.app/Contents/Resources/apollia-os --version
~/Applications/Apollia\ OS.app/Contents/Resources/apollia-os doctor --json | jq .gpu
```

You should see `vendor: Apple`, `recommended_backend: Metal`.

## Embedded components

The macOS bundle contains:

- `llama-server`: the local LLM inference engine (GGUF models, Metal acceleration). The daemon starts and supervises it automatically.
- `apollia-runner-metal` and `apollia-runner-cpu`: the speech-to-text runner (STT, whisper), with Metal acceleration or CPU fallback.

The daemon selects Metal acceleration automatically at startup.

## Update

See [Update Apollia](./mettre-a-jour-apollia.md).

## Uninstall

Drag `Apollia OS.app` to the trash. User data:

```sh
rm -rf ~/.apollia
```
