---
sidebar_position: 6.5
title: Install the desktop app
---

# Install the desktop app

This guide is for people who want to run Apollia as a normal desktop application:
download an installer, install it, and launch. No compiler, no command line, no
source checkout. If instead you want to build from source or run the command-line
runtime, follow [Install and run the runtime](/how-to/install-and-run).

Apollia is local-first. The desktop app runs on your machine, stores its data on
your machine, and does not need an account to start.

## Platform availability

The installers are published on the project's GitHub Releases page. Availability
differs by platform today:

| Platform | Installer | Status |
|---|---|---|
| macOS (Apple Silicon) | `.dmg` | Available, ad-hoc signed. Gatekeeper shows a warning on first launch (see below). |
| Windows (x86-64) | `.msi` / `.exe` | Produced by the release pipeline. Use it from the release assets once published for your version. |
| Linux (x86-64) | `.AppImage` / `.deb` | Produced by the release pipeline. Use it from the release assets once published for your version. |

If your platform's installer is not attached to the latest release yet, build the
app from source with [Install and run the runtime](/how-to/install-and-run), which
covers the developer bring-up on all three operating systems.

## Download

1. Open the releases page: `https://github.com/Apollia-OS/apollia-os/releases`.
2. Pick the latest release.
3. Under **Assets**, download the file that matches your platform:
   - macOS: the `.dmg`
   - Windows: the `.msi` (or the `.exe` installer)
   - Linux: the `.AppImage` or the `.deb`

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

## Install and launch

### macOS

1. Double-click the downloaded `.dmg`.
2. Drag **Apollia OS** into the **Applications** folder.
3. Eject the disk image, then open **Apollia OS** from Applications or Launchpad.

The macOS build is ad-hoc signed, not notarized by an Apple Developer account.
The first time you open it, macOS may say the app "cannot be opened because the
developer cannot be verified". To open it anyway:

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

Windows SmartScreen may warn that the publisher is unrecognized. Choose **More
info**, then **Run anyway** to proceed.

The app renders its interface with Microsoft Edge WebView2. It is preinstalled on
current Windows 11 and updated Windows 10 systems. If the app reports a missing
WebView2 runtime, install the "Evergreen Bootstrapper" from Microsoft's WebView2
Runtime download page, then relaunch.

### Linux

The `.AppImage` is portable and needs no installation:

```sh
chmod +x Apollia_OS_*.AppImage
./Apollia_OS_*.AppImage
```

The `.deb` installs system-wide on Debian and Ubuntu:

```sh
sudo apt install ./apollia-os_*.deb
```

After installing the `.deb`, launch **Apollia OS** from your desktop application
menu. The app relies on the system WebKitGTK runtime; on a minimal system, install
`libwebkit2gtk-4.1-0` if the window fails to open.

## First launch

On first launch the app creates its data directory and walks you through a short
onboarding flow (choosing a model backend, granting permissions). You can start
with a cloud backend or point the app at a local model file you already have. To
run fully local inference, place a `.gguf` model file in `~/.apollia/models/`
before or after onboarding; there is no in-app model downloader.

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
