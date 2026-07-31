# Install Apollia on Linux

Apollia ships for Linux x86_64 in three formats:

- **`.AppImage`** (recommended): portable desktop application, no installation required.
- **`.deb`**: Debian/Ubuntu package (`sudo apt install ./apollia-os_<version>_amd64.deb`).
- **`apollia-os-linux-x86-*.tar.gz`**: CLI bundles per accelerator (CPU, CUDA, ROCm, Vulkan).

## Requirements

- glibc 2.31+ distribution (Ubuntu 22.04, Debian 12, Fedora 38, and so on).
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
sudo apt install ./apollia-os_<version>_amd64.deb
apollia-os start
```

## Verification

```sh
apollia-os --version
apollia-os doctor --json | jq .gpu
```

Expected output:

- NVIDIA RTX → `vendor: Nvidia, recommended_backend: Cuda`
- AMD Radeon → `vendor: Amd, recommended_backend: Rocm`
- Intel/other → `vendor: ..., recommended_backend: Vulkan`

## GPU acceleration

Local LLM inference goes through the embedded `llama-server` engine, shipped with the bundle. Speech-to-text (STT) uses the `apollia-runner` runner, and the AppImage / `.deb` package embeds its CPU variant. To accelerate dictation on GPU:

1. Download the dedicated CLI bundle: `apollia-os-linux-x86-cuda.tar.gz` (or rocm/vulkan).
2. Extract it and copy `apollia-runner-<backend>` next to the `apollia-os` binary.
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
