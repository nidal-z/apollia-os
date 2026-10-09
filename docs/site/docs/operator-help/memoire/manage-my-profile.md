---
title: Manage my profile
slug: /operator-help/memory/manage-my-profile
sidebar_position: 1
---

# Manage my profile

> For operators who want to **review and change**, day to day, what their agents know about them: name, role, function, tools, preferences.
>
> This page covers **editing an existing profile**. If you are launching Apollia for the first time, the guided flow fills the initial values: see **[Set up your profile on first launch](../installation/set-up-your-profile.md)**.

## Why a user profile

A profile describes you once, for every agent that is allowed to read it. When it says you are a developer in fintech, those agents adapt their vocabulary, their tone and their suggestions, with no re-entry for each one.

You can keep **several profiles**, one for each side of your work. One of them is the **default** profile: it is the one your agents use.

An agent reads your profile only when the **Profile** source is checked on its sheet, under **Sources**. An agent without that source knows nothing of it.

The profiles are **local**. No data leaves your machine.

## Where to edit it

In the sidebar, click **Settings**, then the **Profile** section.

The section holds three blocks:

- **Profiles**: the list of your profiles, the default one marked **Default**, and the **New profile** button.
- **Profile details**: the form of the profile selected in the list. Six fields, all optional: **Name**, **Role**, **Function**, **Tools you use**, **Preferences** and **What your agents should know**, a free text.
- **What your agents remembered**: the facts kept in that profile, noted by your agents or added by you.

## Steps

1. Open **Settings**, section **Profile**, and click the profile to edit in the **Profiles** list.

2. Change the fields you want. Nothing is saved while you type.

3. Click **Save** at the bottom of the window (or press **Cmd+S** on macOS, **Ctrl+S** on Windows and Linux). **Discard** puts the fields back as they were.

Selecting another profile saves the changes in progress first.

## Keep several profiles

- **New profile** asks for a name and creates an empty profile.
- The menu at the end of each row offers **Rename**, **Set as default**, **Reset…** and **Delete…**.
- The default profile cannot be deleted: make another profile the default first.
- Deleting a profile deletes its fields, its free text and what your agents remembered in it. This cannot be undone.

These changes take effect at once, without **Save**.

## The facts of a profile

Under the form, **What your agents remembered** lists the facts of the profile. They serve this profile only.

- **Add a fact** adds one by hand, for example *"I invoice my clients on the 5th of the month"*.
- On each fact, **Edit** corrects it, **Copy value** copies it and **Delete** removes it, after a confirmation.
- A chip on each fact says where it comes from: **onboarding** (the guided flow of the first launch), **you**, or **agent**.

A fact is saved at once, without **Save**.

## What an agent may propose

An agent whose **Profile** source is checked may propose a lasting fact about you for your profile. It never writes one on its own:

- The proposal reaches you as a consent request, in the chat and in **To review**, with the fact shown.
- **Once** accepts the fact, which is then added to your profile. **Refuse** drops it.
- Each proposal is asked, every time. There is no "always" for it.

An agent cannot propose a value for one of the six fields of the form this way: those are yours to fill.

## Complete a profile with an agent

When a profile has a name and a role but still has empty fields, a banner offers **Complete with an agent**: an agent asks you a few questions, then fills the form with its proposal. A second banner, *The agent's proposal, to review*, then heads the form. Correct what is off, then click **Save** to keep the proposal or **Skip** to drop it. Nothing is written before **Save**.

## Resetting your profile

Several paths, depending on what you want to erase.

### A - Empty one profile

In the **Profiles** list, open the menu of the profile and choose **Reset…**. The fields, the free text and what your agents remembered in this profile are emptied. The profile stays in the list.

For the default profile, the window offers two buttons: **Reset only**, or **Reset and get acquainted again**, which then starts a short conversation with an agent that proposes a new profile for you to review.

Conversation history and the memories of your agents are not touched.

### B - Take the guided flow again

In **Settings**, section **General**, the **Danger Zone** block holds a **Reset Onboarding** button. It makes the welcome flow of the first launch run again.

It is not neutral for your profile: it also empties what your profile holds, even though the window says no data is deleted. Prefer path A when you only want to change a profile.

### C - Erase all memories (profiles and agents)

In the same **Danger Zone** block, the **Clear Memories** button deletes **all** the memories of Apollia: your profile and the memory of every agent. This is broader than resetting a profile. Use it if you want to start from a completely blank slate on the memory side.

> To go even further (erasing installed agents, conversations, models and settings too), the option is **Factory Reset** at the bottom of the same block.

## Verification

- Close the settings and open them again: the values you saved are there.
- Start a new conversation with an agent whose **Profile** source is checked: it adapts to what the profile says.
- A fact you added carries the **you** chip.

## If it does not work

- **The fields come back as they were**: the change was not saved. Click **Save** before leaving the section.
- **Delete… is refused on a profile**: it is the default profile. Set another one as default first.
- **An agent ignores your profile**: an agent reads your profile only when the **Profile** source is checked for it. Open **Agents**, then the sheet of the agent, and look under **Sources**. From the command line, `apollia-os agent show <agent>` tells you where it stands and `apollia-os agent set <agent> sources profile` checks it (list every source you want kept, the list replaces the one of the package).
- **An agent never proposes a fact**: its **Profile** source is not checked, or it found nothing lasting to propose.

> **Technical reference:** [Apollia reference](/reference)
