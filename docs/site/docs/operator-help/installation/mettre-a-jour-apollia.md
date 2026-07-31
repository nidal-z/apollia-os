---
sidebar_position: 8
title: Update Apollia
---

# Update Apollia

Apollia updates itself from the releases published on GitHub. Two paths, one
per surface. Neither of them touches your data.

## From the application

**Settings > System** shows an update panel, also reachable from
**Settings > About**.

1. Click **Check for updates**. Apollia queries the published releases page
   and compares them with yours.
2. If a newer version exists, it is displayed with its notes.
3. Start the installation. A progress bar tracks the download.
4. Restart the application when it asks you to.

## From the command line

```sh
apollia-os update --check   # looks for something new, installs nothing
apollia-os update           # downloads, verifies, replaces
```

The update runs in three stages, and each one can fail without consequence:

- the binary for your platform is downloaded into a temporary file;
- its SHA256 checksum is verified. **On a mismatch, the operation stops
  without touching the binary in place**;
- the replacement is atomic. You never end up with a half-written binary.

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
- **The replacement fails** on Windows if the application is still running. Close
  Apollia, then run the command again.
- **Nothing new is offered although a version exists**: the artifact for your
  platform may not be attached to that release. Check the published releases
  page and install by hand if needed.
