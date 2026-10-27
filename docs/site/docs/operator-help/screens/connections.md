---
title: The Connections screen
slug: /operator-help/screens/connections
sidebar_position: 4
---

# The Connections screen

> What links your agents to your own accounts, mailboxes, folders and services, and whether each one works.

## What it is for

A connection gives agents tools to work with something of yours: a Microsoft 365 or Google Workspace account, a mailbox, a folder of your computer, or a service from the catalogue. The tools that ship with Apollia are listed here too, under **Apollia, built-in tools**.

The screen has two parts side by side: the list on the left, the card of the connection you chose on the right. Apollia keeps your place, the connection chosen, the search and the filter, when you leave the screen and come back.

## The list

- **Search a connection…** filters the list as you type.
- The kind filter narrows it to **Accounts**, **Mailboxes**, **Services** or **Folders**, or shows **All**.
- What **Needs repair** comes first, then everything else by name.
- Each account connected is a line of its own: two Microsoft accounts make two lines.
- A connection a package installed reads **Brought by the package** followed by the name of the package.

Each line carries the state of its connection:

| State | What it means | The action on its card |
|---|---|---|
| **Working** | The last check found it working, or for a service, its last use. | None needed. |
| **Needs repair** | Something must be done first. The reason is written in plain words, for example *the account must sign in again* or *the folder is no longer where it was*. | The one action that repairs it: **Sign in again**, **Enter the password**, **Choose the folder** or **Start again**. |
| **Paused** | You paused it. Its agents keep their accesses, and none of its tools runs. | **Resume**. |
| **Blocked by your organisation** | Your organisation's policy forbids it, or an administrator must approve it first. | **See the rule**, or, when an administrator must approve it, the approval address and the request to copy, and **Retry**. |
| **Not checked yet** | Nothing has tried it since it was added. | **Check**. |

A state found by a check, a use or a sign-in says when it was found, such as *checked 5 min ago*. **Paused** and **Not checked yet** carry no time. A state changes on screen as soon as Apollia finds it, without reloading the page.

## The card

- **Check** tries the connection now and shows what it found.
- **Try** opens a conversation with Apollia limited to the tools of this connection.
- **Pause** and **Resume**, then **Remove**, which lists what goes with the connection before you confirm. See [Pause or remove a connection](/operator-help/integrations/pause-or-remove-a-connection).
- **Agents that use it** lists the agents that hold its tools, **By default** marking those that hold it through the access every new agent gets. **Give to an agent** gives it to several agents at once.
- **Activity over 7 days**: when it was last used and how many actions each agent took with it. **See in the journal** opens the **Activity log** on this connection alone. A mailbox shares its tools with every other mailbox, so its activity is not counted on its own.
- The tools of the connection, each with its family (read, write or delete, send outside, and so on). For a service added from outside Apollia's selection, **Change** lets you give a tool another family.
- For a service, its settings, such as its arguments, its secrets and its timeouts. How it is launched shows with developer mode on.
- For a Microsoft account, the OneDrive folders and files Apollia may read. For a Google account, the Drive folder its agents work in. For an account, **Add another account** connects a second one of the same provider.

With developer mode on, a **Technical detail** section adds the identifier of the connection, where it comes from and the raw reason of its state, and the card of an account shows the OAuth client it uses.

## Add a connection

**Add a connection** opens one window with a single search. The answers come in four groups: **Offered by Apollia** (the accounts and the services Apollia selected), **On this computer** (a folder, a mailbox), the **Public catalogue**, asked only when you click **Explore all** and marked **Not verified**, and **Connect a service by its address**. Every addition then follows the same three steps: **Connect**, **Who can use it**, **Try it**. See [Connect an MCP server from the catalogue](/operator-help/integrations/connect-an-mcp-server).

## Go further

- [Integrations overview](/operator-help/integrations/integrations-overview)
- [Check a connection and repair it](/operator-help/integrations/test-an-mcp-connection)
- [Pause or remove a connection](/operator-help/integrations/pause-or-remove-a-connection)
- [Connect Microsoft 365](/operator-help/integrations/connect-microsoft-365)
