---
title: Update Apollia
slug: /operator-help/installation/update-apollia
sidebar_position: 8
---

# Update Apollia

Apollia updates itself from the releases published on GitHub. Two paths, one
per surface. Neither of them touches your data.

## Which installs can update themselves

Not every install has an update path, and knowing which one you have saves a
search.

- **The desktop application** updates itself from the macOS `.dmg`, the Linux
  `.AppImage` and the Windows installers. A Linux `.deb`, the CUDA one included,
  has no in-app update: Tauri replaces an AppImage in place and has no
  equivalent for a package, so reinstall the newer `.deb` by hand.
- **The command line** updates itself on four couples: macOS Apple Silicon,
  Linux x86_64, Linux aarch64 and Windows x86_64. macOS Intel has no published
  archive, and the command says so instead of guessing a file name.
- A Vulkan or CUDA install updates from the CPU archive of the same platform.
  That is deliberate: the `apollia-os` binary is identical across the engine
  variants of one platform, and the command replaces that binary only.

## From the application

**Settings > System** shows an update panel, also reachable from
**Settings > About**.

1. Click **Check for updates**. Apollia reads the update manifest attached to
   the latest release and compares its version with yours.
2. If a newer version exists, the panel shows its version number. The manifest
   carries no changelog, so what to expect from that version is on the published
   release page, not in the panel.
3. Start the installation. A progress bar tracks the download.
4. Quit Apollia and open it again once the installation is finished.

## From the command line

```sh
apollia-os update --check   # looks for something new, installs nothing
apollia-os update           # downloads, verifies, replaces
```

The second command asks for a confirmation, `[y/N]`, before it downloads
anything. Pass `--yes` to answer it in advance, in a script for instance.

The update runs in three stages, and each one can fail without consequence:

- the release archive for your platform (named by the bundle contract,
  `apollia-os-<preset>.tar.gz` or `.zip`) is downloaded into a temporary
  directory;
- its SHA256 checksum is verified. **On a mismatch, the operation stops
  without touching the binary in place**;
- the `apollia-os` binary is extracted from the archive and put in place of the
  running one. The bundled Python and runners of your install are left as they
  are.

A lock prevents two simultaneous updates.

## What happens to your data

**Nothing.** The update replaces an executable, not your `~/.apollia`
directory. Your sessions, projects, agents, memory, audit journal and
`apollia.toml` are kept as they are.

One point to know before updating a machine you could not easily reinstall:
**there is no rollback path**. `apollia-os update` takes no target version, it
installs the most recent one, and nothing is provided to reinstall the previous
one nor to bring your databases back to an earlier state. An older version
restarted on data written by a newer one is not a tested case.

If you want to be able to go back, copy `~/.apollia` before the update:

```sh
cp -R ~/.apollia ~/.apollia.backup-$(date +%F)
```

On Windows:

```powershell
Copy-Item -Recurse "$env:USERPROFILE\.apollia" "$env:USERPROFILE\.apollia.backup"
```

## If it does not work

- **"SHA256 mismatch"**: the download was corrupted or interrupted.
  Run it again. Your binary in place was not modified.
- **The replacement fails on Windows.** The command line replaces the executable
  it is itself running from, and Windows refuses to overwrite a file held open
  by a running process. Closing the desktop application does not help, the lock
  being held by the update command. On Windows, take the newer installer from
  the releases page instead.
- **"no release has been published yet"**, or nothing offered although you can
  see a version on GitHub: both paths read the *latest published* release, which
  skips drafts and pre-releases. A release still in draft, or marked as a
  pre-release, is invisible to them until it is published for real.
- **Nothing new is offered although a published release exists**: the artifact
  for your platform may not be attached to that release. Check the published
  releases page, `https://github.com/Apollia-OS/apollia-os/releases`, and
  install by hand if needed.
