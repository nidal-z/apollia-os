---
sidebar_position: 6.5
title: Install the desktop app
description: Install the Apollia desktop application on macOS, Windows or Linux, and get from the download to a first working conversation.
---

# Install the desktop app

This guide is for people who want to run Apollia as a normal desktop application:
download an installer, install it, and launch. No compiler, no command line, no
source checkout. If instead you want to build from source or run the command-line
runtime, follow [Install and run the runtime](/how-to/install-and-run).

Apollia is local-first. The desktop app runs on your machine, stores its data on
your machine, and does not need an account to start.

## Platform availability

Three platforms are supported. Installers are published on the project's GitHub
Releases page:

| Platform | Installer | Notes |
|---|---|---|
| macOS (Apple Silicon) | `.dmg` | Gatekeeper shows a warning on first launch unless the build is Developer ID signed (see below). |
| Windows (x86-64) | `.msi` / `.exe` | SmartScreen warns until the build is Authenticode signed. Needs the WebView2 runtime, preinstalled on current Windows and downloaded by the installer otherwise. |
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
| macOS (Apple Silicon) | `Apollia OS_0.1.0-1_aarch64.dmg` |
| Linux (x86-64) | `Apollia OS_0.1.0-1_amd64.AppImage`, `Apollia OS_0.1.0-1_amd64.deb` |
| Windows (x86-64) | `Apollia OS_0.1.0-1_x64_en-US.msi`, `Apollia OS_0.1.0-1_x64-setup.exe` |
| Linux (x86-64), CUDA engine | `Apollia OS_0.1.0-1_amd64-cuda.deb` |
| Windows (x86-64), CUDA engine | `Apollia OS_0.1.0-1_x64_en-US-cuda.msi`, `Apollia OS_0.1.0-1_x64-setup-cuda.exe` |
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

Every published file also carries a detached Sigstore signature (`.sig`) and
its signing certificate (`.pem`), produced by the release pipeline with
keyless `cosign`. To verify the origin of a download, not only its integrity:

```sh
cosign verify-blob <file> \
  --certificate <file>.pem --signature <file>.sig \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com \
  --certificate-identity-regexp '^https://github.com/Apollia-OS/'
```

`cosign` is a separate tool from the Sigstore project; the runtime does not
embed it, and `apollia-os update` verifies the SHA256 checksum only.

## Install and launch

### macOS

1. Double-click the downloaded `.dmg`.
2. Drag **Apollia OS** into the **Applications** folder.
3. Eject the disk image, then open **Apollia OS** from Applications or Launchpad.

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

Apollia OS requires macOS 13 (Ventura) or newer.

### Windows

1. Double-click the `.msi` (or `.exe`) installer.
2. Follow the installer steps, then launch **Apollia OS** from the Start menu.

Windows SmartScreen may warn that the publisher is unrecognized. The installer
names Apollia as its publisher, but it is not signed with an Authenticode
certificate yet, which is what SmartScreen checks. Choose **More info**, then
**Run anyway** to proceed.

The app renders its interface with Microsoft Edge WebView2. It is preinstalled on
current Windows 11 and updated Windows 10 systems. Where it is missing, the
installer downloads it, so that step needs a network connection. If the app
reports a missing WebView2 runtime, install the "Evergreen Bootstrapper" from
Microsoft's WebView2 Runtime download page, then relaunch.

The window close button quits the app on Windows, stopping the runtime and the
background inference and speech-to-text processes with it. Use the tray icon to
keep it running while hiding the window. macOS is the exception, where closing a
window leaves the app resident behind the menu-bar icon.

### Linux

The `.AppImage` is portable and needs no installation. Its file name contains
a space, so quote it:

```sh
chmod +x "Apollia OS_"*.AppImage
"./Apollia OS_"*.AppImage
```

The `.deb` installs system-wide on Debian and Ubuntu:

```sh
sudo apt install "./Apollia OS_"*.deb
```

After installing the `.deb`, launch **Apollia OS** from your desktop application
menu. The app relies on the system WebKitGTK runtime; on a minimal system, install
`libwebkit2gtk-4.1-0` if the window fails to open.

## First launch

On first launch the app creates its data directory and walks you through a short
onboarding flow (choosing a model backend, granting permissions). You can start
with a cloud backend or point the app at a local model file you already have. To
<!-- claim:desktop-downloads-models-in-app -->
run fully local inference, download a GGUF from within the app: onboarding offers
it, and Settings, Model Hub does the same afterwards. Dropping a `.gguf` file
into `~/.apollia/models/` by hand also works, before or after onboarding.

## Where the app stores your data

Everything the app persists lives under a single directory in your home folder:

| Path | Contents |
|---|---|
| `~/.apollia/` | The app's data root, created on first run. |
| `~/.apollia/models/` | Local `.gguf` model files you provide. |
| `~/.apollia/api-token` | The bearer token for the local HTTP API. |

Nothing leaves your machine unless you explicitly configure a cloud backend or a
connector. To reset the app to a clean state, quit it and remove `~/.apollia/`
(this deletes your agents, memory, and configuration, so back it up first if you
care about it).

On Windows the data root is the `.apollia` folder inside your user profile
(`%USERPROFILE%\.apollia`).

## Next steps

- Understand the runtime behind the app: [Install and run the runtime](/how-to/install-and-run).
- Speed up local inference: [Accelerate local inference](/how-to/accelerate-local-inference).
- Write your own agent: [Your first agent](/tutorials/your-first-agent).
