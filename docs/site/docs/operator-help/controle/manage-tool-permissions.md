---
title: Manage tool permissions
slug: /operator-help/control/manage-tool-permissions
sidebar_position: 2
---

# Manage tool permissions

> For operators who want to review what each agent may do on this machine, and withdraw a consent, without waiting for an action to raise a new request.

## Prerequisites

- The application is open and at least one agent is installed. The Apollia agent, the one of the Chat, is always listed.

## How are permissions created?

The **Permissions** screen gathers three things for each agent:

- **Its accesses, by family.** They come from the sheet of the agent: what it was granted when it was installed or created, and what you adjusted since. The families are **Read**, **Read personal data**, **Write or delete**, **Send outside** and **Run code**.
- **Its "always".** Each one is a consent you gave by answering **Always for this agent** to a request (see [Approve or reject an agent action](approve-or-reject-an-action.md)). It covers one family of action, for that agent only.
- **Its fine rules.** A fine rule allows or refuses one precise action. One is written when you answer **Always allow this command** to a request to run code, and others can be written by hand with the developer mode on.

A permission has one of two scopes and no other: **one agent**, or **every agent**. An "always" is always for one agent. A fine rule is for one agent or for every agent.

## Review the permissions of an agent

1. In the sidebar, click **Permissions**.

2. At the top, the **Organization policy** panel says whether your organization deposited a policy on this machine, and what it imposes. Without one, it reads *No policy: you are free.*

3. The list on the left has two parts:
   - **For every agent**, with the row **Rules for every agent**.
   - **Each agent**, one row per agent, the Apollia agent first. Under each name, a line counts its families, its "always" and its fine rules.

   The field **Filter by agent, family or access…** above the list narrows it.

4. Click an agent. The panel on the right shows:
   - **Accesses granted, by family**: each family the agent holds an access in, what that access lets it do, and a badge that says what the family asks before the agent acts: **Without asking** for reading, **Your consent is asked**, **Always granted** when an "always" covers it, **Consent every time** for running code, or **Enforced by policy** when the policy of your organization forbids the family.
   - **"Always" given**: each standing consent, with the date it was given.
   - **Fine rules**: each rule in plain words, with a badge **Allows** or **Refuses**, its family, whom it targets and the date it was added.

   **Open the sheet** leads to the sheet of the agent, where its accesses are adjusted.

<!-- claim:prefix-rules-evaluated-per-invocation -->
> **What a fine rule actually does:** a fine rule is checked on every action of the agents it targets, against the request itself. A rule that **refuses** always wins: the action it matches is refused, even when an "always" would have let it through. A rule that **allows** a command lets that command run without asking, while the developer mode is on.
<!-- claim:executor-guard-blocks-command-chaining -->
> **Code** is stricter: a rule that allows only applies to a **single simple command** that starts with the text of the rule, with no chaining (`;`, `&&`, `||`), pipe, redirection (`>`, `<`) or substitution (`` ` ``, `$(...)`). A rule with no command never lets code run without asking.

## Withdraw a consent or a rule

1. Select the agent, or **Rules for every agent**.
2. On the line of the "always" or of the fine rule, click **Withdraw**.
3. The withdrawal takes effect at once and is recorded in the Activity log. The line stays in place, marked **Withdrawn**, with a **Cancel** button that puts it back, until you leave the screen.

Once an "always" is withdrawn, the next action of that family by that agent asks for your consent again.

## Revoke everything at once

1. Click **Revoke all** above the list.
2. Under **What is withdrawn**, choose **The fine rules only**, **The "always" consents only** or **Both**.
3. Under **Scope**, choose **Every agent** or **One agent**, and pick the agent.
4. Read the line that says how many fine rules and how many "always" are concerned, then click **Revoke all**.

This is immediate and cannot be undone. The accesses granted by family do not change: they are adjusted on the sheet of each agent.

## Write a fine rule (developer mode)

With the developer mode on (**Settings**, section **Developer**), the screen also carries an **Add a fine rule** button, and each fine rule gets **Edit** and **Duplicate**. The window asks for:

- **Effect**: **Allow** or **Refuse**.
- **Action family** and **Tool**: the tool the rule is about.
- **Argument prefix**: the start of the request the rule matches, for example `git status`. For a tool that runs code, allowing requires a simple command here.
- **Scope**: **Every agent** or **One agent**.

Every rule written or changed is recorded in the Activity log. Everyone sees the rules and can withdraw them, with the developer mode off as well.

## The built-in tools

The tools shipped with Apollia (web search, page reading, Python and the others) are not turned on or set up here. A line at the bottom of the screen says so, and **Open Connections** leads to **Connections**, section **Built-in tools**. See [Inspect a tool](inspect-a-tool.md).

## Review the decisions taken

This screen shows what is in force, not what happened. Each withdrawal and each rule written is recorded in the **Activity log**, which you open from the sidebar; its **Nature** filter separates **Actions** from **Access changes**. See [Read the audit trail](../observabilite/read-the-audit-trail.md).

## If it does not work

- **An agent shows no "always" and no fine rule**: none was given. Its accesses by family still apply, and each sensitive action asks for your consent.
- **The Withdraw button is missing on a family**: an access granted by family is not withdrawn here. Click **Open the sheet** and adjust the accesses of the agent.
- **The Revoke all button of the window is greyed out**: the choice you made concerns no fine rule and no "always".
- **An agent that is no longer installed is listed**: it still holds an "always" or a fine rule, which you can withdraw.
- **Add a fine rule is not shown**: the developer mode is off.

> **Technical reference:** [Apollia reference](/reference)
