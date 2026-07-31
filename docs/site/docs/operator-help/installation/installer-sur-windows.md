# Install Apollia on Windows

Apollia ships for Windows x86_64 in three formats:

- **`.msi`** (recommended): standard Windows installer with Start menu entries + uninstaller.
- **`.exe` (NSIS)**: portable single-file installer.
- **`apollia-os-windows-x86-*.zip`**: CLI bundles per accelerator (CPU, CUDA, Vulkan).

## Requirements

- Windows 10 22H2 / Windows 11.
- 4 GB of free RAM minimum.
- For GPU: NVIDIA 550+ driver (CUDA) or a Vulkan-capable driver.
- **No need** to install the Visual C++ Redistributable: the CRT is statically embedded.

## Installation (MSI)

1. Download `Apollia-OS_<version>_x64.msi`.
2. Double-click, follow the wizard.
3. The app shows up in the Start menu.

The Windows firewall will ask you to allow `apollia-os.exe`, the `llama-server.exe` inference engine and `apollia-runner-*.exe` (speech-to-text) on first launch: **allow them on private networks** (these components talk to the daemon over loopback 127.0.0.1).

## Verification

From PowerShell:

```powershell
& "C:\Program Files\Apollia OS\apollia-os.exe" --version
& "C:\Program Files\Apollia OS\apollia-os.exe" doctor --json | ConvertFrom-Json | Select-Object -ExpandProperty gpu
```

## GPU acceleration

Local LLM inference goes through the embedded `llama-server` engine, shipped with the bundle. Speech-to-text (STT) uses the `apollia-runner` runner, and the MSI installer embeds its CPU variant. To accelerate dictation on a CUDA / Vulkan GPU:

1. Download `apollia-os-windows-x86-cuda.zip` (or vulkan).
2. Extract it and copy `apollia-runner-cuda.exe` (or `apollia-runner-vulkan.exe`) into `C:\Program Files\Apollia OS\`.
3. Restart the app.

## What is different on Windows

Windows is a supported platform, but two points set it apart from the other
two and are worth knowing before you hand a task to an agent.

**No tool confinement.** On Linux, a command started by an agent runs in
isolated namespaces and under resource limits; on macOS, under resource limits.
On Windows, neither: a command started by an agent runs with exactly your
rights, on your files, with no memory or CPU time cap. The practical
consequence: on Windows, only run agents whose code you have read, and keep
manual approval enabled in the chat.

**The shell tool requires a POSIX shell.** `bash_executor` looks for an `sh` in
your `PATH`. Without Git Bash, WSL or MSYS2 installed, any agent that uses that
tool fails. The other tools, files, web and Python, work normally.

## Update

See [Update Apollia](./mettre-a-jour-apollia.md).

## Uninstall

`Settings > Apps > Apollia OS > Uninstall`. User data:

```powershell
Remove-Item -Recurse "$env:USERPROFILE\.apollia"
```
