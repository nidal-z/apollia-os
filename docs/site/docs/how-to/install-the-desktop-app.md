---
sidebar_position: 6.5
title: Install Apollia Desktop
description: Install Apollia Desktop, the desktop application, on macOS, Windows or Linux, and get from the download to a first working conversation.
---

# Install Apollia Desktop

This guide is for people who want to run Apollia Desktop, the desktop
application, like any other:
download an installer, install it, and launch. No compiler, no command line, no
source checkout. If instead you want to build from source or run the command-line
runtime, follow [Install and run the runtime](/how-to/install-and-run).

Apollia is local-first. Apollia Desktop runs on your machine, stores its data on
your machine, and does not need an account to start.

## Which file carries which name

The application is **Apollia Desktop**: that is the name in its window, in its
menus and in this documentation. The files that install it, and what they put
on disk, still carry the name of the engine underneath, Apollia OS.

| What you see | Its name |
|---|---|
| The application: window, menus, help | Apollia Desktop |
| The installers on the release page | `Apollia-OS_<version>_...` |
| The application file on macOS | `Apollia OS.app` |
| The Start menu entry and the install folder on Windows | `Apollia OS` |
| The application menu entry and the resource folder on Linux | `Apollia OS`, `/usr/lib/Apollia OS/` |
| The command line shipped inside | `apollia-os` |

## Platform availability

Three platforms are supported. Installers are published on the project's GitHub
Releases page:

| Platform | Installer | Notes |
|---|---|---|
| macOS (Apple Silicon) | `.dmg` | Gatekeeper shows a warning on first launch unless the build is Developer ID signed (see below). |
| Windows (x86-64) | `.msi` / `.exe` | SmartScreen warns when the build carries no Authenticode signature (see below). Needs the WebView2 runtime, preinstalled on current Windows and downloaded by the installer otherwise. |
| Linux (x86-64) | `.AppImage` / `.deb` | Needs WebKitGTK, present on current desktop distributions. |

Tool confinement is not uniform across the three, and that difference is not
cosmetic: see [what is confined and what is
not](/explanation/agent-trust-model). On Windows there is none.

If your platform's installer is not attached to the release you are looking at,
build the app from source with
[Install and run the runtime](/how-to/install-and-run), which
covers the developer bring-up on all three operating systems.

## Download

1. Open the releases page: `https://github.com/Apollia-OS/apollia-os/releases`.
2. Pick the latest release.
3. Under **Assets**, download the file that matches your platform:
   - macOS: the `.dmg`
   - Windows: the `.msi` (or the `.exe` installer)
   - Linux: the `.AppImage` or the `.deb`

The file names carry the product name with a space, exactly as the bundler
writes them:

<!-- release-artifacts:begin - generated from packaging/artifacts.json by docs/site/regen.sh; do not edit by hand -->
| Platform | Files on the release page |
|---|---|
| macOS (Apple Silicon) | `Apollia-OS_0.3.1_aarch64.dmg` |
| Linux (x86-64) | `Apollia-OS_0.3.1_amd64.AppImage`, `Apollia-OS_0.3.1_amd64.deb` |
| Windows (x86-64) | `Apollia-OS_0.3.1_x64_en-US.msi`, `Apollia-OS_0.3.1_x64-setup.exe` |
<!-- release-artifacts:end -->

Each release also attaches a `SHA256SUMS` file. To confirm your download is
intact, compare its checksum against that file.

```sh
# macOS / Linux
shasum -a 256 <downloaded-file>
```

```powershell
# Windows (PowerShell)
Get-FileHash .\<downloaded-file> -Algorithm SHA256
```

Every published file also carries a Sigstore bundle (`.cosign.bundle`), which
holds its signature and its signing certificate together, produced by the
release pipeline with keyless `cosign`. To verify the origin of a download, not
only its integrity:

```sh
cosign verify-blob <file> \
  --bundle <file>.cosign.bundle \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com \
  --certificate-identity-regexp '^https://github.com/Apollia-OS/'
```

`cosign` is a separate tool from the Sigstore project; the runtime does not
embed it, and `apollia-os update` verifies the SHA256 checksum only.

## Install and launch

### macOS

1. Double-click the downloaded `.dmg`.
2. Drag **Apollia OS**, the name the application file carries, into the
   **Applications** folder.
3. Eject the disk image, then open it from Applications or Launchpad.

A macOS build is signed and notarized with an Apple Developer ID when the release
pipeline has the signing secrets, and ad-hoc signed otherwise. If yours is
ad-hoc, the first launch may say the app "cannot be opened because the developer
cannot be verified". To open it anyway:

- Right-click (or Control-click) the app icon and choose **Open**, then confirm
  **Open** in the dialog. macOS remembers the choice for later launches.
- If the app is still blocked, allow it once from the Terminal:

  ```sh
  xattr -dr com.apple.quarantine "/Applications/Apollia OS.app"
  ```

Apollia Desktop requires macOS 13 (Ventura) or newer. Models in the GGUF
format run on the embedded llama.cpp engine on every supported version; models
in the MLX format, served by the MLX engine the macOS bundle ships, need macOS 14
or newer.

### Windows

1. Double-click the `.msi` (or `.exe`) installer.
2. Follow the installer steps, then launch the application from the Start menu,
   under the entry **Apollia OS**.

Windows SmartScreen may warn that the publisher is unrecognized. The installer
names Apollia as its publisher, but SmartScreen checks an Authenticode
signature, which the release pipeline applies only when it holds the signing
certificate. On an unsigned build, choose **More info**, then **Run anyway** to
proceed.

The app renders its interface with Microsoft Edge WebView2. It is preinstalled on
current Windows 11 and updated Windows 10 systems. Where it is missing, the
installer downloads it, so that step needs a network connection. If the app
reports a missing WebView2 runtime, install the "Evergreen Bootstrapper" from
Microsoft's WebView2 Runtime download page, then relaunch.

The window close button quits the app on Windows, stopping the runtime and the
background inference and speech-to-text processes with it. The tray icon opens
the window, shows the pending approvals and quits; it does not hide the window
while keeping the runtime up, so minimise the window instead. To keep agents
running with no window at all, turn the developer mode on and use the switch of
**Settings > Developer** that installs the runtime as a service of your
session. macOS is the exception, where closing a window leaves the app resident
behind the menu-bar icon.

### Linux

The `.AppImage` is portable and needs no installation. Its file name contains
a space, so quote it:

```sh
chmod +x Apollia-OS_*.AppImage
./Apollia-OS_*.AppImage
```

The `.deb` installs system-wide on Debian and Ubuntu:

```sh
sudo apt install ./Apollia-OS_*.deb
```

After installing the `.deb`, launch the application from your desktop
application menu, under the entry **Apollia OS**. The app relies on the system WebKitGTK runtime; on a minimal system, install
`libwebkit2gtk-4.1-0` if the window fails to open.

## First launch

On first launch the app creates its data directory, `~/.apollia/`, and opens a
first-launch window. It counts seven steps; the top of the window shows where
you are, and **Configure later**, at the top right, closes it at any step.

1. **Language**: French or English. Everything after this step follows it.
2. **AI**: where the AI of your agents runs. This is the only step you cannot
   skip.
   - **On this computer**: Apollia lists the `.gguf` files it already finds (in
     `~/.apollia/models/`, `~/Downloads/` and the LM Studio models folder) and
     a list of **Recommended models**, sized for your machine. Clicking one
     starts its download; you can go on with the next steps while it arrives.
     **Search on HuggingFace** looks for another model.
   - **Use a server**: the address of an OpenAI-compatible server your
     organisation runs or that you already use (Ollama, vLLM, LM Studio, a
     LiteLLM gateway), its key if it asks for one, then **Test** and the model
     your agents will use.
3. **Search, voice**: two optional, smaller models. One lets your agents find
   a document by its meaning; the other transcribes your voice for dictation.
   Each is downloaded or imported from a file, then tried on the spot.
4. **About me**: the model you chose answers one question, so you see it work,
   then a short form takes what your agents should know about you. Every field
   is optional.
5. **First agent**: install a package you received (a `.apo` file), create an
   agent by describing your need in a few sentences, or start from an example
   Apollia proposes. This step can be skipped.
6. **Connection**: shown only when that agent needs a connection you do not
   have yet, such as a mailbox or a folder. It can be skipped.
7. **First task**: the window closes on your agent at work, a conversation
   with its first example already written, or, for the example that qualifies
   requests, a batch on a sample workbook. Without an agent, a conversation with
   Apollia, the built-in agent, opens.

Every step is detailed in
[Set up your profile on first launch](/operator-help/installation/set-up-your-profile).
The flow resumes where you left it after a restart, and it can be replayed from
the **Danger Zone** of **Settings > General**.

<!-- claim:desktop-downloads-models-in-app -->
To run fully local inference, download a model from within the app: the first
launch offers it, and the AI screen, **Discover models**, does the same
afterwards. Dropping a `.gguf` file into `~/.apollia/models/` by hand also
works, before or after the first launch. Apollia picks the engine from the
format of the model: GGUF on llama.cpp, MLX on the MLX engine of Apple Silicon.

## The Python interpreter agents run on

Apollia ships with its own Python 3.13, and that is what agents run on: nothing
has to be installed on the machine, and no administrator rights are needed. The
interpreters already on the machine are not consulted, whatever their version.

If your environment requires a different one, name it in the settings, under
**System > Hardware** with the developer mode on, or in `apollia.toml`:

```toml
[tools]
python_interpreter = "/opt/corp/python3.13/bin/python3"
```

It is checked before it is accepted: it has to exist, start, and be a Python
3.13, because the standard library that ships with Apollia cannot be read by
another minor version. If it later stops satisfying that, after an upgrade or an
uninstall, the bundled interpreter takes over and the log says why, rather than
the agent's tools failing.

The same key is writable from a shell, with the same check:

```sh
apollia-os config set tools.python_interpreter /opt/corp/python3.13/bin/python3
```

## Where the app stores your data

Everything the app persists lives under a single directory in your home folder:

| Path | Contents |
|---|---|
| `~/.apollia/` | The app's data root, created on first run. |
| `~/.apollia/apollia.toml` | The configuration file, the one every surface reads and writes. |
| `~/.apollia/models/` | The local models, downloaded from the app or placed there by hand. |
| `~/.apollia/api-token` | The bearer token for the local HTTP API. |

Nothing leaves your machine unless you explicitly declare a server or add a
connection. To reset the app to a clean state, quit it and remove `~/.apollia/`
(this deletes your agents, memory, and configuration, so back it up first if you
care about it).

On Windows the data root is the `.apollia` folder inside your user profile
(`%USERPROFILE%\.apollia`).

## Coming from 0.2.0-preview {#coming-from-0-2-0-preview}

The installers keep the name Apollia OS, so 0.3.0 replaces an installed 0.2.0
in place: use **Install and restart** in the app, or run the new installer. The
databases under `~/.apollia` are migrated at the first start of 0.3.0, and
0.2.0 refuses to open them afterwards: copy the folder first if you may go back.
What changes for you, and what to do, is in
[Update Apollia](/operator-help/installation/update-apollia#coming-from-0-2-0-preview).

## Next steps

- Understand the runtime behind the app: [Install and run the runtime](/how-to/install-and-run).
- Speed up local inference: [Accelerate local inference](/how-to/accelerate-local-inference).
- Install, create or adjust an agent from the app: [Install an agent](/operator-help/agents/install-an-agent).
- Write your own package, in code: [Your first agent](/tutorials/your-first-agent).
