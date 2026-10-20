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
  `.AppImage` and the Windows installers. A Linux `.deb` has no in-app update:
  Tauri replaces an AppImage in place and has no equivalent for a package, so
  reinstall the newer `.deb` by hand.
- **An install made from a former `-cuda` installer** updates like any other,
  to the one installer of its system; a `-cuda.deb` is replaced by hand with
  the `.deb`. That installer no longer carries the engine for NVIDIA cards:
  after the update, the AI screen offers it, **Speed up with your NVIDIA
  card**, and one click brings it back.
- **The command line** updates itself on four couples: macOS Apple Silicon,
  Linux x86_64, Linux aarch64 and Windows x86_64. macOS Intel has no published
  archive, and the command says so instead of guessing a file name.
- A Vulkan install updates from the CPU archive of the same platform.
  That is deliberate: the `apollia-os` binary is identical across the engine
  variants of one platform, and the command replaces that binary only.

## From the application

**Settings > General** opens on the **App update** block: the installed
version, the update panel and the **Update channel**, **Stable** or
**Preview**.

1. Click **Check for updates**. Apollia reads the update manifest attached to
   the latest release and compares its version with yours.
2. If a newer version exists, the panel shows its version number, and under
   **What's new** the notes of that version when the release carries some.
   Otherwise what to expect from that version is on the published release page.
3. Click **Install and restart**. A progress bar tracks the download.
4. Once the installation is finished, Apollia closes and opens again on the new version.

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

**Nothing is removed.** The update replaces an executable, not your
`~/.apollia` directory. Your sessions, sources, agents, memory, audit journal
and `apollia.toml` are kept. A new version may migrate the databases of that
directory at its first start, as 0.3.0 does.

One point to know before updating a machine you could not easily reinstall:
**there is no rollback path**. `apollia-os update` takes no target version, it
installs the most recent one, and nothing is provided to reinstall the previous
one nor to bring your databases back to an earlier state. Once 0.3.0 has
migrated them, 0.2.0 refuses to open a database whose schema is newer than its
own.

If you want to be able to go back, copy `~/.apollia` before the update:

```sh
cp -R ~/.apollia ~/.apollia.backup-$(date +%F)
```

On Windows:

```powershell
Copy-Item -Recurse "$env:USERPROFILE\.apollia" "$env:USERPROFILE\.apollia.backup"
```

## Coming from 0.2.0-preview {#coming-from-0-2-0-preview}

The installers and the installed application keep the name Apollia OS, so
0.3.0 updates a 0.2.0 installation in place rather than installing beside it;
the window and the interface say Apollia Desktop. An installation made from a
former `-cuda` installer is brought to the standard installer of its system,
and the AI screen then offers the accelerated NVIDIA engine. Copy `~/.apollia`
first, as above, if you may go back to 0.2.0.

At its first start, 0.3.0 converts what 0.2.0 left, and a few things ask a
gesture from you:

- **Cloud providers.** The Anthropic, Mistral and OpenAI cloud backends are
  deleted, and the default moves to the local engine when there is one. A
  server you run yourself is kept. To keep reaching a hosted model, put a
  gateway you operate in front of it (LiteLLM, for instance) and declare its
  address on the AI screen, as in
  [Connect a remote model](connecter-un-modele-distant.md).
- **Projects.** Each project becomes sources grouped under its name, its
  working folder a folder connection granted to no agent, and its conversations
  keep their agent. Anything that could not be imported is listed in a report,
  shown once in the app and readable with `apollia-os source report`. Grant
  the imported folders to the agents that need them, on their sheets. See
  [What became of projects](../projets/what-became-of-projects.md).
- **Operator and Builder.** They are replaced by one switch, the developer
  mode, in **Settings > Developer**, off by default. A former Builder choice
  turns it on.
- **Folders.** An agent with no folder granted no longer reaches your home
  folder. Grant it a folder connection, for reading or for writing, on its
  sheet.
- **Standing consents.** Rules scoped to a project or a session are converted
  once and journaled; a standing allow on a code executor, or on a tool of
  unknown family, is removed and will be asked again.
- **Agents written in Python.** An agent is no longer written in Python: an
  agent that was is rewritten as a package, its mission and accesses as a
  record, its code as tools and workflows.
- **Configuration.** Only `~/.apollia/apollia.toml` is read. A file left in
  the working directory or under `~/.config/apollia` is ignored, and
  `apollia-os doctor` names it with the command that moves it. Keys that 0.3.0
  withdrew still load, with a warning, and do nothing.

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
  pre-release, is invisible to them until it is published for real. In the
  application, the **Preview** update channel also receives the trial versions.
- **Nothing new is offered although a published release exists**: the artifact
  for your platform may not be attached to that release. Check the published
  releases page, `https://github.com/Apollia-OS/apollia-os/releases`, and
  install by hand if needed.
