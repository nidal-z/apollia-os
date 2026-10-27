---
title: Pause or remove a connection
slug: /operator-help/integrations/pause-or-remove-a-connection
sidebar_position: 6.5
---

# Pause or remove a connection

> For any operator who wants to stop a connection for a while without losing anything, or to take it out for good.

## Prerequisites

- At least one connection in **Connections**.

## Pause, or remove?

| | Pause | Remove |
|---|---|---|
| The agents' accesses | Kept as they are. | Taken away from every agent. |
| Its tools | None runs until you resume it. | Gone. |
| Its settings, secrets and sign-in | Kept. | Erased. |
| Undo | **Resume**, and it works as it did before. | Add the connection again, then give it back to the agents. |

## Pause a connection

1. In **Connections**, open the card of the connection.
2. Click **Pause**. The state becomes **Paused**, and the card says that its agents keep their accesses but that none of its tools runs until it is resumed.

While it is paused:

- An agent that calls one of its tools is refused, and told that the connection is paused. The refusal is written in the **Activity log**: *An action of the agent was refused: its connection is paused*, for a mailbox as for an account, a folder or a service.
- A folder is neither read nor searched: an agent that reads, lists or writes a file in it, or asks the context of a code folder it holds, is refused and told that the connection is paused, with the same line in the **Activity log**; an agent that searches its sources is told that this one is paused, and the other sources it holds are searched.
- The sheet of each agent that holds it marks the access **Paused**.
- A service is stopped and never started again, neither at the next launch of Apollia nor on request. In the Chat, the **@** picker no longer offers its resources.
- The pause holds across a restart of Apollia.

A connection **Blocked by your organisation** offers no pause: the policy already stops it.

## Resume a connection

On the card, click **Resume**. The list and the card show its new state at once. Its tools run again for the agents that held them, with no access to give back, and a service is started again.

## Remove a connection

1. In **Connections**, open the card of the connection.
2. Click **Remove**. Before anything is erased, the card lists what goes with the connection, one line each:
   - its settings,
   - the files installed for it,
   - its sign-in to the account,
   - each secret kept for it,
   - each tool an agent held through it, with the name of the agent,
   - for a folder, the source and its index,
   - for a Microsoft account, each OneDrive folder or file chosen for it.
3. Click **Remove the connection** to confirm, or **Cancel**.

The connection leaves the list. If something could not be erased, a message names it, so you can remove it by hand. Removing a Microsoft account also forgets the OneDrive folders and files you had chosen for it.

Removing an account erases its sign-in on this computer only. To withdraw Apollia's access on the provider's side too, remove the Apollia application from your account settings: https://myaccount.microsoft.com for Microsoft, https://myaccount.google.com/permissions for Google.

## Verification

- After **Pause**, the line of the connection reads **Paused**, and asking an agent to use it gets a refusal that says the connection is paused.
- After **Resume**, the line reads its state again, and the agent's request goes through.
- After **Remove**, the connection is no longer in the list, and the agents' sheets no longer show it under **Accesses**.

## If it does not work

- **A paused connection still seems to answer**: check that the agent did not use another connection of the same kind. Two accounts are two connections, each paused on its own.
- **The removal says something could not be erased**: the message names it. A secret left in the system keychain can be deleted from the keychain tool of your system.

> **Technical reference:** [Apollia reference](/reference) , connection routes, governance, audit journal.
