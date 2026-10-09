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

1. In the sidebar, click **Settings**. The window opens on the **General** section, which holds everything this page uses.

2. The **App update** block shows the **Installed version**, and under it the update channel in force: **Stable channel** or **Preview channel**. The channel is the one chosen on the next line, **Update channel**; it is not guessed from the version number. Click the version to copy it.

<!-- claim:about-reports-resolved-data-dir -->

3. The **Application** block shows the **Data directory**: the single folder that holds your conversations, agent memory, models, configuration and audit journal. Click it to copy the full path.

   > **Note:** the path shown is the one this installation actually resolved, not a generic example. It normally reads `.apollia` inside your home directory, and it follows the home directory Apollia was launched with. Trust the value on screen over any path written in a guide.

4. At the bottom of the section, the **About** block holds **What runs on this machine**, which states, item by item, what stays local: inference, voice transcription, storage and the audit journal. The licence, the open-source components Apollia is built on and the links to the documentation and the source code follow.

## Report a problem with the right information

1. In **Settings**, open **Hardware** (in the **System** group) and click **Copy the information** in its header. This copies a plain-text block with the version, the operating system, the processor, the memory, each GPU, and each engine with its version. In developer mode it also carries the Python interpreter, the configuration file and the data directory.

2. Back in **General**, in the **Application** block, click **Open in browser** on the **Report a problem** line. Your browser opens the reporting page.

3. Paste that block into the report, then describe what you were doing and what you expected instead.

> **Nothing is sent automatically.** The block goes to your clipboard and nowhere else. Read it before pasting it, and remove any path you would rather not publish.

## Back up or erase everything

The data directory from step 3 is a normal folder. Copying it elsewhere backs Apollia up; deleting it puts the application back to its first launch. For the guided path, with its precautions, see [Reset Apollia (factory reset)](../troubleshooting/factory-reset.md).

## Going further

The full manual, with the guides for every screen, is published on **docs.apollia.fr**. Reach it from **Settings**, section **General**: **Open the help center** in the **Help** block, or **Documentation** under **Resources** in the **About** block.

## Verification

The **General** section shows a version, and the data directory it displays exists on your disk under that exact path.

## If it does not work

- **The version reads "Version unknown":** the interface reached the section before the engine was ready. Close the settings and open them again, or relaunch the application.
- **The data directory says the home directory could not be resolved:** Apollia was launched in an environment with no home directory (`HOME` on macOS and Linux, `USERPROFILE` on Windows). This happens with some service launchers. Relaunch it from your normal session.
