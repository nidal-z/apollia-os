---
title: Install Apollia on Windows
slug: /operator-help/installation/install-on-windows
sidebar_position: 2
---

# Install Apollia on Windows

Every file below is attached to each release on the publication page,
`https://github.com/Apollia-OS/apollia-os/releases`.

- **`Apollia-OS_<version>_x64_en-US.msi`** (recommended): standard Windows installer with Start menu entries + uninstaller.
- **`Apollia-OS_<version>_x64-setup.exe`**: the NSIS installer. A single file, and it installs rather than runs in place: it registers an uninstaller in **Settings > Apps** exactly as the `.msi` does.
- **`Apollia-OS_<version>_x64_en-US-cuda.msi`** and **`Apollia-OS_<version>_x64-setup-cuda.exe`**: the same two installers with a CUDA build of the inference engine, for an NVIDIA card.
- **`apollia-os-windows-x86-cpu.zip`** and **`apollia-os-windows-x86-vulkan.zip`**: the two command-line bundles, one per engine, CPU or Vulkan.

## Requirements

- Windows 10 22H2 / Windows 11.
- 4 GB of free RAM minimum.
- For GPU inference: a Vulkan-capable driver, or the NVIDIA 550+ driver if you take the CUDA bundle.
- **No need** to install the Visual C++ Redistributable: the CRT is statically embedded.

## Installation (MSI)

1. Download `Apollia-OS_<version>_x64_en-US.msi`.
2. Double-click, follow the wizard. It names the product, the publisher
   (Apollia), the version and the dual MIT / Apache-2.0 licence.
3. The app shows up in the Start menu.

The installer is not signed with an Authenticode certificate yet, so SmartScreen
warns about an unrecognised publisher: choose **More info**, then **Run anyway**.

The Windows firewall will ask you to allow `apollia-os.exe`, the `llama-server.exe` inference engine and `apollia-runner-*.exe` (speech-to-text) on first launch: **allow them on private networks** (these components talk to the daemon over loopback 127.0.0.1).

The installer downloads the WebView2 runtime if your machine does not already
have it, so the install step needs a network connection on a machine that has
never run a WebView2 application. Current Windows 11 ships it preinstalled.

## Installation (.exe)

The NSIS installer asks who the installation is for. **Install for all users**
puts it in `C:\Program Files\Apollia OS\`, the same place as the `.msi`.
**Install for me only** needs no administrator right and puts it under your user
profile instead. Note the destination the wizard shows you: the verification
below reads from it.

## Closing the app

<!-- claim:desktop-close-button-quits-outside-macos -->
The window close button quits Apollia on Windows, the way it does for any other
Windows application. The runtime stops with it, and so do the two background
processes it owns: the `llama-server.exe` inference engine and the
`apollia-runner-*.exe` speech-to-text runner. macOS behaves differently, where
closing a window is not the same gesture as quitting: there the app stays
resident behind the menu-bar icon.

<!-- claim:desktop-exit-stops-inference-engine -->
Quitting always stops the inference engine, whichever surface you quit from: the
close button, the tray menu, or the in-app menu. Nothing is left holding video
memory or a loopback port afterwards. To confirm, from PowerShell after
quitting:

```powershell
Get-Process apollia-os, llama-server, apollia-runner-cpu -ErrorAction SilentlyContinue
```

It should print nothing.

<!-- claim:windows-no-console-window -->
You should never see a terminal window belonging to Apollia. The background
processes are console programs, and they are started with the Windows
`CREATE_NO_WINDOW` flag so none of them opens a console of its own. If a terminal
window does appear, that is a defect worth reporting rather than something to
close by hand.

## Verification

From PowerShell. The path below is the all-users one; on a per-user `.exe`
install, replace it with the destination the wizard showed you.

```powershell
& "C:\Program Files\Apollia OS\apollia-os.exe" --version
& "C:\Program Files\Apollia OS\apollia-os.exe" doctor
```

`doctor` runs eight checks: the data directory, the configuration file, the two
databases, the models directory, Python, the sandbox posture and the runtime
socket. It does **not** detect your GPU. The command that reports the detected
hardware is a separate one, and it needs the daemon running:

```powershell
& "C:\Program Files\Apollia OS\apollia-os.exe" model hardware --json
```

It answers with the total RAM, the CPU and the detected accelerator. Ask for
`--json`: the plain-text rendering of that command prints nothing today.

## What Windows does not confine

<!-- claim:windows-has-no-tool-sandbox -->
On Linux, a tool call runs inside namespaces with resource limits. On Windows
there is **no confinement at all**: no namespaces, and no resource limits either,
because the Unix mechanism has no Windows equivalent and the function that
applies it does nothing on this platform. A tool an agent runs has the same
rights over your machine as the application itself.

That does not make Windows unusable, and it does change what you should delegate.
The permission rules and the approval prompts still apply, and they are the only
barrier here, so treat an "always allow" on Windows as a wider grant than the
same choice on Linux.

One practical consequence: `bash_executor` needs a POSIX shell on `PATH`, from
Git Bash, WSL or MSYS2, and fails without one.

## GPU acceleration

Two accelerations, and Windows answers them differently.

**Local LLM inference already runs on the GPU.** The installers embed a Vulkan
build of the `llama-server` engine, and the `-cuda` installers a CUDA build for
NVIDIA cards; the `apollia-os-windows-x86-vulkan.zip` command-line bundle carries
the Vulkan one. Vulkan drives NVIDIA, AMD and Intel cards alike. Nothing to
install and nothing to set: with a working driver the engine uses the card. The
`-cpu` bundle is the one to take on a machine with no graphics driver.

**Dictation stays on the CPU.** Speech to text runs in the `apollia-runner`
sidecar, which is built on whisper, and whisper has no Vulkan backend: the
`apollia-runner-vulkan.exe` binary shipped in the Vulkan archive is byte for byte
the CPU one. Copying it into the installation directory changes nothing. No
Windows artifact published today carries a GPU-accelerated speech-to-text runner.

## What is different on Windows

Windows is a supported platform, but three points set it apart from the other
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

**A named pipe instead of a Unix socket.** Elsewhere the command line reaches
the runtime through a Unix socket that the filesystem protects at `0600`. There
is no such socket on Windows, so the runtime serves a named pipe,
`\\.\pipe\apollia-runtime-<user>`, and the command line opens it. Two
consequences for you. The global `--socket` option is accepted and ignored: the
pipe name is derived from your `USERNAME`, not from a path you choose. And the
pipe is created with a default security descriptor, weaker than the socket's
`0600`, so what protects the runtime here is the bearer token in
`%USERPROFILE%\.apollia\api-token`. Treat that file as a password: any account
that can read it can drive your runtime.

## Update

See [Update Apollia](./mettre-a-jour-apollia.md).

## Uninstall

`Settings > Apps > Apollia OS > Uninstall`, for the `.msi` and the `.exe` alike.
User data:

```powershell
Remove-Item -Recurse "$env:USERPROFILE\.apollia"
```
