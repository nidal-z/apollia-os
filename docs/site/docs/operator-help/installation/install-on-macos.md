---
title: Install Apollia on macOS
slug: /operator-help/installation/install-on-macos
sidebar_position: 1
---

# Install Apollia on macOS

Apollia ships for macOS from the publication page,
`https://github.com/Apollia-OS/apollia-os/releases`, which attaches three files
for this platform:

- **`Apollia-OS_<version>_aarch64.dmg`** (recommended): full desktop application, drag it into `Applications`.
- **`apollia-os-macos-silicon.tar.gz`**: CLI bundle only (power users).
- **`Apollia-OS.app.tar.gz`**: the payload the in-app updater downloads, with its `.sig` signature. It is not an install format; ignore it for a first install.

## Requirements

- macOS 13 (Ventura) or newer.
- Apple Silicon (M1, M2, M3, M4). Intel support through Rosetta is not official for v0.1.0.
- 4 GB of free RAM minimum. The curated list offered at onboarding is Qwen3 in four sizes, 4B, 8B, 14B and 30B-A3B; 8 GB is a comfortable floor for the 8B.
- 10 GB of disk space for the bundle + one quantized model.

## Installation (DMG)

1. Download `Apollia-OS_<version>_aarch64.dmg` from the releases page above.
2. Double-click the file, drag `Apollia OS.app` into your **personal** `Applications` folder, the one under your home directory. The verification commands below use that path; installing into the system-wide `/Applications` works too, in which case adjust them.
3. First launch: the release is signed and notarized with an Apple Developer ID when the pipeline holds the signing secrets, and signed ad hoc otherwise. An ad-hoc build is refused on the first double-click. Right-click (or Control-click) the icon, choose **Open**, then confirm **Open** in the dialog; macOS remembers the choice. If it is still blocked, clear the quarantine flag once from a terminal:

   ```sh
   xattr -dr com.apple.quarantine ~/Applications/Apollia\ OS.app
   ```

4. The app starts the `apollia-os` daemon automatically. The daemon serves local LLM inference through the embedded `llama-server` engine and launches the speech-to-text (STT) runner.

## Verification

From a terminal:

```sh
~/Applications/Apollia\ OS.app/Contents/Resources/apollia-os --version
~/Applications/Apollia\ OS.app/Contents/Resources/apollia-os doctor
```

`doctor` checks the data directory, the configuration file, the two databases,
the models directory, Python, the sandbox posture and the runtime socket. It does
**not** detect your GPU. The command that reports the detected hardware is a
separate one, and it needs the daemon running:

```sh
~/Applications/Apollia\ OS.app/Contents/Resources/apollia-os model hardware --json
```

It answers with the total RAM, the CPU and the detected accelerator, probed from
the machine. Ask for `--json`: the plain-text rendering of that command prints
nothing today.

## Closing the app

Closing the window with the red button hides it and leaves Apollia running
behind the menu-bar icon, which is the macOS convention: the daemon, the
`llama-server` engine and the runner all stay up, and the menu-bar icon reopens
the window. Quitting for real goes through `Cmd+Q`, the application menu, or
**Quit** in the menu-bar icon, and that stops every background process.

Windows and Linux differ here: the close button quits outright on both.

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
