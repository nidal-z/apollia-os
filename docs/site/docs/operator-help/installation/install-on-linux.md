---
title: Install Apollia on Linux
slug: /operator-help/installation/install-on-linux
sidebar_position: 3
---

# Install Apollia on Linux

Every file below is attached to each release on the publication page,
`https://github.com/Apollia-OS/apollia-os/releases`.

- **`.AppImage`** (recommended, x86_64): portable desktop application, no installation required.
- **`.deb`** (x86_64): Debian/Ubuntu package (`sudo apt install ./Apollia-OS_<version>_amd64.deb`).
- **`Apollia-OS_<version>_amd64-cuda.deb`**: the same package with a CUDA build of the inference engine, for an NVIDIA card. No CUDA AppImage is published.
- **`apollia-os-linux-x86-cpu.tar.gz`** and **`apollia-os-linux-x86-vulkan.tar.gz`**: the two command-line bundles for x86_64, one per engine, CPU or Vulkan.
- **`apollia-os-linux-arm-cpu.tar.gz`**: the command-line bundle for aarch64. No desktop installer is published for that architecture.

## Requirements

- glibc 2.39+ distribution (Ubuntu 24.04, Debian 13, Fedora 40, and so on): the
  released binaries are built on Ubuntu 24.04 without static linking.
- 4 GB of free RAM minimum.
- For GPU inference: a Vulkan 1.3+ driver, Mesa for AMD and Intel, the NVIDIA
  550+ driver for NVIDIA.

## Installation (AppImage)

```sh
chmod +x Apollia-OS_<version>_amd64.AppImage
./Apollia-OS_<version>_amd64.AppImage
```

The app starts the daemon in the background. The daemon serves local LLM inference through the embedded `llama-server` engine and launches the speech-to-text (STT) runner.

One thing to know before you pick this format: an AppImage is a read-only mount
that exists only while the app runs. The command line lives inside that mount,
and the `/usr/local/bin` link offered by **Settings > System** points into it, so
the link dangles as soon as you quit. For a command line that survives, install
the `.deb` or unpack one of the `apollia-os-linux-*` archives.

## Installation (.deb)

```sh
sudo apt install ./Apollia-OS_<version>_amd64.deb
```

Then launch **Apollia OS** from your desktop application menu: the app starts
the daemon itself. The `apollia-os` command line ships inside the package
(`/usr/lib/apollia-os/`) but is not on your `PATH` until you enable it from
**Settings > System** in the app, which creates the `/usr/local/bin` link. The
verification commands below assume that link exists.

## Verification

```sh
apollia-os --version
apollia-os doctor
```

`doctor` checks the data directory, the configuration file, the two databases,
the models directory, Python, the sandbox posture and the runtime socket. It
does **not** detect your GPU. The command that reports the detected hardware is
a separate one, and it needs the daemon running:

```sh
apollia-os model hardware --json
```

It answers with the total RAM, the CPU and the detected accelerator. Ask for
`--json`: the plain-text rendering of that command prints nothing today.

## Closing the app

The window close button quits Apollia on Linux, and the daemon stops with it,
along with the `llama-server` engine and the `apollia-runner-*` runner. The tray
icon carries three entries, open the window, show the pending approvals, and
quit; none of them hides the window while keeping the runtime up, so minimise
the window rather than close it when you want Apollia to stay resident. macOS is
the exception: there, closing a window leaves the app running behind the
menu-bar icon.

## GPU acceleration

Two accelerations, and Linux answers them differently.

**Local LLM inference already runs on the GPU.** The desktop bundles, AppImage
and `.deb` alike, embed a Vulkan build of the `llama-server` engine, the
`-cuda.deb` a CUDA build for NVIDIA cards, and the
`apollia-os-linux-x86-vulkan.tar.gz` command-line bundle the Vulkan one. Vulkan drives
NVIDIA, AMD and Intel cards alike. Nothing to install and nothing to set: with a
working driver the engine uses the card. The `-cpu` bundle is the one to take on
a machine with no graphics driver.

**Dictation stays on the CPU.** Speech to text runs in the `apollia-runner`
sidecar, which is built on whisper, and whisper has no Vulkan backend: the
`apollia-runner-vulkan` binary shipped in the Vulkan archive is byte for byte
the CPU one. Copying it over the bundled runner changes nothing. No Linux
artifact published today carries a GPU-accelerated speech-to-text runner.

## Update

See [Update Apollia](./mettre-a-jour-apollia.md).

## Uninstall

```sh
sudo apt remove apollia-os    # .deb package
# or simply delete the AppImage
rm -rf ~/.apollia              # user data
```
