---
title: Check a connection and repair it
slug: /operator-help/integrations/test-an-mcp-connection
sidebar_position: 6
---

# Check a connection and repair it

> For any operator who wants to know whether a connection really works, and to put it right when it does not.

## Prerequisites

- At least one connection in **Connections**: an account, a mailbox, a folder or a service.

## Check a connection

1. In **Connections**, open the card of the connection.
2. Click **Check**. Apollia tries the connection now: it signs in to an account, opens the mailbox, looks for the folder, or starts the service and asks for its tools. A service that was asleep is started for the check.
3. The state at the top of the card shows what was found, with the time: *checked just now*.

For a service, its use also updates its state: when its calls fail it turns to **Needs repair**, and when they work again it turns back to **Working**. The page learns it at once, with nothing to reload.

## Read the state

| State | What it means |
|---|---|
| **Working** | The last check found it working, or for a service, its last use. |
| **Needs repair** | Something must be done first. The reason says what, in plain words. |
| **Paused** | You paused it. See [Pause or remove a connection](pause-or-remove-a-connection.md). |
| **Blocked by your organisation** | Your organisation's policy forbids it, or an administrator must approve it first. |
| **Not checked yet** | Nothing has tried it since it was added. It is listed with the others, not at the top. |

A state is only **Working** once Apollia has seen it work. With developer mode on, the **Technical detail** section of the card also shows the raw reason of the state.

## Repair it

The card offers the one action that repairs the connection, under its state:

| Reason | Action | What it does |
|---|---|---|
| *the account must sign in again* | **Sign in again** | Opens the sign-in to the account, as when it was added. A remote service that signs in through its own page does the same. |
| *an administrator must approve it first* (Microsoft 365) | **Retry** | The card shows the approval address and a request written for your administrator, each ready to copy (**Copy the address**, **Copy the request**). Once your administrator approved, **Retry** opens the sign-in again. |
| *the server refuses the password* | **Enter the password** | Opens the mailbox form on that address, for a new app password. |
| *the folder is no longer where it was* | **Choose the folder** | Lets you pick the folder again. The connection stays the same, so its agents keep their accesses. |
| *unreachable* or *its operations fail* | **Start again** | Starts the service again. |
| None shown, and no approval address: the state reads **Blocked by your organisation** | **See the rule** | Opens **Permissions**. |

Once a folder is chosen again, the agents search its new content straight away: nothing from the former folder comes back in their answers, even while the new one is still being read.

For a work Microsoft account that an administrator must approve, see the **Blocked by your organisation** entry of [Connect Microsoft 365](connecter-microsoft-365.md).

## Error messages when a service is added

When you add a service and its trial fails, Apollia turns the technical error into a clear message:

- *"Authentication refused - check your API key."*: invalid or expired key.
- *"Access forbidden - your key doesn't have the required rights."*: the key lacks a permission.
- *"Service not found - check the URL or server name."*: wrong address or missing server.
- *"Network error - the service did not respond in time."*: the service could not be reached.
- *"Command not found - the package is probably not installed."*: a local service whose program is missing.
- *"The connection failed. Check your credentials and try again."*: any other failure.

**Show technical details** reveals the raw message, **Retry** tries again and **Remove** takes the connection out.

## Verification

- The state reads **Working**, with a recent time.
- The tools you expect are listed on the card.
- If some tools of a service are missing, look at Apollia first: it keeps at most `max_tools` tools per server, 256 by default, and logs `mcp.tools.bounded` with what it kept and what it received when it cuts. A server response is also capped at `max_response_bytes`. Only once both are ruled out does the service itself become the explanation.

## If it does not work

- **The state stays Needs repair after the action**: click **Check** again, then read the reason. With developer mode on, **Technical detail** gives the raw message.
- **A local service is unreachable**: its program is not in Apollia's PATH, or it stops at once. Check the command with developer mode on.
- **It works, but an agent does not use it**: the agent does not hold its tools. On the card, **Give to an agent**, or see [Understand the scope of an integration](understand-integration-scope.md).

> **Technical reference:** [Apollia reference](/reference) , connection states, MCP handshake semantics.
