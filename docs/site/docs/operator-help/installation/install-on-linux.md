---
title: Install Apollia Desktop on Linux
slug: /operator-help/installation/install-on-linux
sidebar_position: 3
---

# Install Apollia Desktop on Linux

The application is Apollia Desktop. Its installers and the files they put on
disk still carry the name of the engine underneath, Apollia OS, which is the
name you will read below.

Every file below is attached to each release on the publication page,
`https://github.com/Apollia-OS/apollia-os/releases`.

- **`.AppImage`** (recommended, x86_64): portable desktop application, no installation required.
- **`.deb`** (x86_64): Debian/Ubuntu package (`sudo apt install ./Apollia-OS_<version>_amd64.deb`).
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

Then launch the application from your desktop application menu, under the
entry **Apollia OS**: the app starts
the daemon itself. The `apollia-os` command line ships inside the package
(`/usr/lib/Apollia OS/`) but is not on your `PATH` until you enable it from
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
and `.deb` alike, embed a Vulkan build of the `llama-server` engine, and the
`apollia-os-linux-x86-vulkan.tar.gz` command-line bundle the Vulkan one. Vulkan drives
NVIDIA, AMD and Intel cards alike. Nothing to install and nothing to set: with a
working driver the engine uses the card. The `-cpu` bundle is the one to take on
a machine with no graphics driver.

**An NVIDIA card can go further.** There is one installer per system, and no
separate one for NVIDIA cards. When the application detects such a card, its AI
screen offers the accelerated engine, **Speed up with your NVIDIA card**: one
click downloads it, after its signature is verified, and the next model load
uses it. On Linux that engine relies on the CUDA libraries of the machine.

**Dictation runs on the processor in the desktop packages.** Speech to text
runs in the `apollia-runner` sidecar, which is built on whisper. The desktop
packages ship a single one, `apollia-runner-cpu`; the Vulkan command-line
archive carries `apollia-runner-vulkan`, whisper on the card, which the daemon
uses when the driver provides a Vulkan loader.

## Update

See [Update Apollia](./mettre-a-jour-apollia.md).

## Uninstall

```sh
sudo apt remove apollia-os    # .deb package
# or simply delete the AppImage
rm -rf ~/.apollia              # user data
```
