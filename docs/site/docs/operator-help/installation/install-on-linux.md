---
title: Install Apollia on Linux
slug: /operator-help/installation/install-on-linux
sidebar_position: 3
---

# Install Apollia on Linux

Apollia ships for Linux x86_64 in three formats:

- **`.AppImage`** (recommended): portable desktop application, no installation required.
- **`.deb`**: Debian/Ubuntu package (`sudo apt install ./Apollia-OS_<version>_amd64.deb`).
- **`apollia-os-linux-x86-*.tar.gz`**: CLI bundles per accelerator (CPU, CUDA, ROCm, Vulkan).

## Requirements

- glibc 2.39+ distribution (Ubuntu 24.04, Debian 13, Fedora 40, and so on): the
  released binaries are built on Ubuntu 24.04 without static linking.
- 4 GB of free RAM minimum.
- For GPU: up-to-date driver (NVIDIA 550+, ROCm 6.0+, or Mesa Vulkan 1.3+).

## Installation (AppImage)

```sh
chmod +x Apollia-OS_<version>_amd64.AppImage
./Apollia-OS_<version>_amd64.AppImage
```

The app starts the daemon in the background. The daemon serves local LLM inference through the embedded `llama-server` engine and launches the speech-to-text (STT) runner suited to your GPU.

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
does **not** detect your GPU, and there is no command that reports one: the
inference device is chosen by configuration rather than probed. On Linux it
defaults to CPU, so GPU acceleration is something you set, not something you
verify here. See the section below.

## Closing the app

The window close button quits Apollia on Linux, and the daemon stops with it,
along with the `llama-server` engine and the `apollia-runner-*` runner. To keep
the runtime resident while hiding the window, use the tray icon rather than the
close button. macOS is the exception: there, closing a window leaves the app
running behind the menu-bar icon.

## GPU acceleration

Local LLM inference goes through the embedded `llama-server` engine, shipped with the bundle. Speech-to-text (STT) uses the `apollia-runner` runner, and the AppImage / `.deb` package embeds its CPU variant. To accelerate dictation on GPU:

1. Download the GPU CLI bundle: `apollia-os-linux-x86-vulkan.tar.gz`. Vulkan drives NVIDIA, AMD and Intel cards alike; the `-cpu` bundle is the one to take on a machine with no graphics driver.
2. Extract it and copy `apollia-runner-<backend>` into the installation's
   `runners/` directory (for the `.deb`, `/usr/lib/apollia-os/runners/`, owned
   by root, so use `sudo cp`).
3. Restart: `apollia-os stop && apollia-os start`.

The daemon detects the added runner automatically.

## Update

See [Update Apollia](./mettre-a-jour-apollia.md).

## Uninstall

```sh
sudo apt remove apollia-os    # .deb package
# or simply delete the AppImage
rm -rf ~/.apollia              # user data
```
