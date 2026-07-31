# Manage my profile

> For operators who want to **review and change**, day to day, what all their agents know about them: first name, role, sector, supervision, constraints.
>
> This page covers **editing an existing profile**. If you are launching Apollia for the first time, the guided flow fills the initial values: see **[Set up your profile on first launch](../installation/configurer-votre-profil.md)**.

## Why a user profile

All your agents share a single **user profile**. When you tell one of them that you are a developer in fintech, the other agents benefit from that information: matching vocabulary, adjusted tone, relevant suggestions. No re-entry for each agent.

This profile is **local**. No data leaves your machine.

## Where to edit it

**Settings → Profile**, reachable from the ⚙️ icon in the sidebar.

![Settings then Profile page, showing its stacked sections from Identity down to the danger zone](/img/operator-help/en/memoire-gerer-mon-profil-1.png)

The screen is split into sections:

- **Identity** - first name, role, sector, team size, goals.
- **Agent supervision** *(sensitive)* - HITL level (Human-in-the-Loop), autonomy areas, triggering.
- **Tools & business context** - everyday tools, technical level, connected integrations *(the last one is sensitive)*.
- **Constraints** *(sensitive)* - data sovereignty, compliance.
- **Preferences** - preferred language, default LLM backend.

## Steps

1. Open **Settings → Profile**.

2. Enter or change the fields you want. Each field is saved automatically when you leave it (focus out) or change an option. A toast confirms the save.

3. Emptying a field and leaving the focus **deletes** the profile entry.

## Who filled each field

A **chip** appears next to the fields already filled in and shows where the value came from:

- **onboarding** - value set by the guided flow at first start. You will see it on the few fields the onboarding fills (first name, role, supervision level, data sovereignty).
- **you** - value entered or changed from this form. Any change you make here replaces the previous origin with **you**.
- **agent** - value inferred by an agent during a conversation (for example an agent that noticed your role in an exchange).

An empty field carries no chip. As long as you do not touch a field, its origin chip is preserved, focusing it without changing anything does not replace it.

## Sensitive fields

Four fields carry a **Sensitive** badge:

- **HITL level** (Agent supervision)
- **Data sovereignty** (Constraints)
- **Compliance** (Constraints)
- **Connected integrations** (Tools & business context)

Changing these fields **does not automatically re-apply** your permission rules. Apollia honours the principle "memory does not change the environment without an explicit decision". For the new values to influence permissions, rerun the onboarding (see below).

## Resetting your profile

Several paths, depending on what you want to erase.

### A - Erase only the profile and ask the questions again

At the bottom of the **Settings → Profile** page, the **Danger zone** offers a **Reset profile** button. Confirm in the modal.

![The profile danger zone with the Reset profile confirmation modal in the foreground](/img/operator-help/en/memoire-gerer-mon-profil-1bis.png)

- The whole profile is erased (all 5 sections).
- The onboarding agent restarts **immediately** to rebuild your preferences from scratch.
- Conversation history and the memories of other agents **are not touched**.

This is the option to pick if you only want to "take the configuration questionnaire again" without touching the rest.

### B - Take the guided flow again (without erasing the profile)

In **Settings → Danger Zone** (a separate entry in the Settings sidebar), the **Reset Onboarding** button restarts the guided flow without erasing what is already filled in. Useful if you only want to re-download a model, recalibrate an integration, or see the welcome screens again.

### C - Erase all memories (profile + agents + projects)

In **Settings → Danger Zone**, the **Clear Memories** button deletes **all** Apollia memories, across every namespace: user profile, agent memory, project memory. This is broader than resetting the profile, use it if you want to start from a completely blank slate on the memory side (conversations, installed agents and permissions stay in place).

> To go even further (erasing installed agents, permissions and system settings too), the option is **Factory Reset** at the bottom of the same Danger Zone.

## Verification

- Reload the page: the values you entered persist.
- Start a new conversation with any agent and notice that it addresses you by your first name / adapts its tone to your role.
- On a field you just changed, the chip switches to **you**.

## If it does not work

- **Fields stay empty after entry**: an agent may have been writing. Try again after a few seconds.
- **The "Reset profile" button does not restart the onboarding**: open the sidebar and click **Onboarding** to restart it manually.
- **An agent ignores your profile**: all Python agents have access to the profile by default. If the agent in question is third-party, open its code (or contact its author) to check that it reads `ctx.profile`.

> **Technical reference:** [Apollia reference](/reference) - canonical field schema, single source of truth for the global profile, Python SDK contract `ctx.profile.*`.
