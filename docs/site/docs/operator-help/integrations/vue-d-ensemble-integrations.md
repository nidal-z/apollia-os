---
title: Integrations overview
slug: /operator-help/integrations/integrations-overview
sidebar_position: 1
---

# Integrations overview

> For any operator who wants to understand Apollia's two extension mechanisms, native connectors and MCP servers, and know where to start.

## Prerequisites

- Apollia running, the **Connections** page reachable from the sidebar.
- An account with the service you want to plug in (Google, Microsoft, Notion, etc.) if the integration is authenticated.

## The two families

Apollia distinguishes two complementary mechanisms.

### Native OAuth connectors

Maintained directly by Apollia for services that do not (yet) expose an official MCP server: **Google Workspace** (Gmail, Calendar, Drive, Sheets, Docs, Slides, Forms, Tasks) and **Microsoft 365** (Outlook, Calendar, OneDrive).

- Tokens stored in the system keyring (macOS Keychain, Windows Credential Manager, Linux Secret Service).
- Direct calls from your machine to `gmail.googleapis.com` or `graph.microsoft.com`, no Apollia cloud relay.
- Every write asks for your agreement before it runs.
- Several accounts can be connected. Each one is a connection of its own, granted to an agent account by account.

**The two do not cost the same to start.** Worth knowing before you click, because the difference shows up only once you start connecting:

| | What it takes to connect |
|---|---|
| **Microsoft 365** | Nothing. Apollia ships the identifier of its own registered application, so you sign in and you are done. |
| **Google Workspace** | About ten minutes in the Google Cloud console first. You register your own OAuth client and hand its credentials to Apollia, with developer mode on. |

This is not an oversight on the Google side. Google requires a verified consent screen before an application may serve accounts outside its own project, and its desktop clients also carry a secret that no distributed binary can hold. Microsoft's public desktop clients require neither. [Connect Google Workspace](connecter-google-workspace.md) explains what the Testing status costs, notably a reconnection about once a week, and [Set up a Google OAuth client](/how-to/set-up-a-google-oauth-client) names every console screen.

### MCP servers

The open Model Context Protocol standard. Third-party processes, local (stdio via `npx` or `uvx`) or remote (HTTP/SSE), which expose tools consumable by any MCP client. **Add a connection** opens one window with a single search, whose answers come in four groups: **Offered by Apollia** (the Google Workspace and Microsoft 365 accounts, then the selection of services, each entry with its label, its description and an **Official** or **Community** badge), **On this computer** (a folder, a mailbox), the **Public catalogue** and **Connect a service by its address**. The public MCP registry is read only when you click **Explore all**, and its entries are marked **Not verified**; they show in batches, and **Show N more** under the list shows the next ones.

Apollia reads its selection from a signed index, online, when the window opens, so an entry can be added or corrected without a new version of Apollia. An index whose signature does not hold is refused, and the copy already kept on this computer, or the selection built into Apollia, is shown instead. The setting `[mcp] catalogue_enabled = false` turns the online index off. The selection holds these entries:

Notion, Linear, Atlassian (Jira + Confluence), Stripe, Figma (Dev Mode), Sentry, Cloudflare, PostgreSQL, Time, Fetch, Memory (Knowledge Graph), Brave Search, Web Browser.

You can also add your own server, with **Connect a service by its address**, always last in the window. With developer mode on, the same form also takes a local command. Changing the catalogue itself is not possible from the interface.

## Where to start

- Mail, calendar, personal or work drive: see [Connect Google Workspace](connecter-google-workspace.md) or [Connect Microsoft 365](connecter-microsoft-365.md).
- Notion, Linear, Atlassian, Stripe, etc.: see [Connect an MCP server](connecter-un-serveur-mcp.md).
- Your internal MCP servers: see [Wire your own MCP server](cabler-son-propre-serveur-mcp.md).
- A connection already in place: see [Check a connection and repair it](test-an-mcp-connection.md) and [Pause or remove a connection](pause-or-remove-a-connection.md).
- Tailoring the catalogue to your team is not possible.

## How to choose

| Service | Native connector | Official MCP | Recommendation |
|---|---|---|---|
| Gmail, Google Calendar, Drive | Apollia | None | Native connector |
| Outlook, Calendar, OneDrive | Apollia | None | Native connector |
| Notion, Linear | None | Official | Catalogue |
| Atlassian (Jira + Confluence) | None | Atlassian Rovo | Catalogue |
| Stripe, Figma, Sentry, Cloudflare | None | Official | Catalogue |
| Your internal server | None | To be wired | Connect a service by its address |

## Staying in control

- **Your agreement first**: every write (sending mail, creating an event, writing a file) asks for your agreement before execution. See [Understand MCP permissions](understand-mcp-permissions.md).
- **Local tokens**: tokens are kept on your machine and sent only to the service they belong to. See [Manage OAuth tokens](manage-oauth-tokens.md).
- **Organisation policy**: forbidding the cloud for connections is a rule of the signed policy your organisation may deploy, `connections.remote = "deny"`, checked by the runtime. Under it, Apollia refuses to add a connection to an online service, a Google or Microsoft account among them, and says so in the connection dialog. Without a policy you are free: no connection is refused for being online. The policy is read when Apollia starts: a connection it forbids, already in place, then reads **Blocked by your organisation** and its tools are refused.
- **Pause**: a connection you pause keeps its agents' accesses, and none of its tools runs until you resume it. See [Pause or remove a connection](pause-or-remove-a-connection.md).
- **Secrets in the secret store**: a key typed when a service is added, a header, and a value whose name marks it secret go to the secret store of this computer, never in the settings of the connection.

## Verification

- The **Connections** page opens and lists your connections, each with its state.
- **Add a connection** opens a window with one search, and its four groups of answers.

## If it does not work

- **The Connections page shows a banner**: a banner names what could not be read, such as the list of agents or the descriptions of the catalogue, and the rest of the page stays. Restart Apollia if it does not go away.
- **Connecting a native connector stops with "Refused by your organisation's policy"**: your organisation's policy denies connections to online services on this computer. Nothing was sent; ask the person who manages the policy for an exception.

> **Technical reference:** [Apollia reference](/reference) , Tool Registry architecture, scoping, tool governance.
