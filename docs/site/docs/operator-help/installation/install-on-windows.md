---
title: Install Apollia on Windows
slug: /operator-help/installation/install-on-windows
sidebar_position: 2
---

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

From PowerShell:

```powershell
& "C:\Program Files\Apollia OS\apollia-os.exe" --version
& "C:\Program Files\Apollia OS\apollia-os.exe" doctor
```

`doctor` runs eight checks: the data directory, the configuration file, the two
databases, the models directory, Python, the sandbox posture and the runtime
socket. It does **not** detect your GPU, and there is no command that reports
one: the inference device is chosen by configuration rather than probed. See the
section below.

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

Local LLM inference goes through the embedded `llama-server` engine, shipped with the bundle. Speech-to-text (STT) uses the `apollia-runner` runner, and the MSI installer embeds its CPU variant. To accelerate dictation on a CUDA / Vulkan GPU:

1. Download `apollia-os-windows-x86-vulkan.zip`. Vulkan drives NVIDIA, AMD and Intel cards alike; the `-cpu` bundle is the one to take on a machine with no graphics driver.
2. Extract it and copy `apollia-runner-vulkan.exe` into `C:\Program Files\Apollia OS\`.
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
