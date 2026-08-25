---
title: Find your version and your data
slug: /operator-help/transversal/find-your-version-and-data
sidebar_position: 5
---

# Find your version and your data

> For operators who need to know exactly what is installed, where Apollia keeps their data on disk, and what to attach to a bug report.

## Prerequisites

- None. The page is available at any time, including offline.

## Steps

1. In the sidebar, click **Settings**, then **About** (bottom of the navigation, in the **Help** cluster).

2. The header shows the **version**, the **release channel** and the **platform**. The channel is read from the version string: a version carrying a suffix after the hyphen, such as `0.1.0-preview`, is a preview build; a bare version is a stable one.

3. The **Version and build** section lists the values that identify this installation: version, platform, Python interpreter, inference engine and transcription engine. Click any value to copy it.

<!-- claim:about-reports-resolved-data-dir -->

4. The **Where your data lives** section shows the **data directory**: the single folder that holds your conversations, agent memory, models, configuration and audit journal. Click it to copy the full path.

   > **Note:** the path shown is the one this installation actually resolved, not a generic example. It normally reads `.apollia` inside your home directory, and it follows the home directory Apollia was launched with. Trust the value on screen over any path written in a guide.

5. The **What runs on this machine** section states, item by item, what stays local: inference, voice transcription, storage and the audit journal.

## Report a problem with the right information

1. Still on the **About** page, in the **Version and build** section, click **Copy diagnostic report**. This copies a plain-text block with the version, the platform, the Python interpreter, the data directory, both engines and the license.

2. In the **Resources** section, click **Report a problem**. Your browser opens a new issue on the public repository.

3. Paste the diagnostic report into the issue, then describe what you were doing and what you expected instead.

> **Nothing is sent automatically.** The diagnostic report goes to your clipboard and nowhere else. Read it before pasting it, and remove any path you would rather not publish.

## Back up or erase everything

The data directory from step 4 is a normal folder. Copying it elsewhere backs Apollia up; deleting it puts the application back to its first launch. For the guided path, with its precautions, see [Reset Apollia (factory reset)](../troubleshooting/reinitialiser-apollia-factory-reset.md).

## Going further

The full manual, with the guides for every screen, is published on **docs.apollia.fr**. Reach it from **Settings → Help → Help center**, or from **Settings → About → Documentation**.

## Verification

The **About** page shows a version, and the data directory it displays exists on your disk under that exact path.

## If it does not work

- **"System information could not be loaded":** the interface reached the page before the runtime was ready. Leave the page and come back, or relaunch the application.
- **The data directory says the home directory could not be resolved:** Apollia was launched in an environment with no home directory (`HOME` on macOS and Linux, `USERPROFILE` on Windows). This happens with some service launchers. Relaunch it from your normal session.
- **The version reads "Version unknown":** same cause as the first case. The rest of the page stays usable.
