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

**The two do not cost the same to start.** Worth knowing before you click, because the difference shows up only once you are on the connector page:

| | What it takes to connect |
|---|---|
| **Microsoft 365** | Nothing. Apollia ships the identifier of its own registered application, so you sign in and you are done. |
| **Google Workspace** | About ten minutes in the Google Cloud console first. You register your own OAuth client and hand its credentials to Apollia. |

This is not an oversight on the Google side. Google requires a verified consent screen before an application may serve accounts outside its own project, and its desktop clients also carry a secret that no distributed binary can hold. Microsoft's public desktop clients require neither. [Connect Google Workspace](connecter-google-workspace.md) explains what the Testing status costs, notably a reconnection about once a week, and [Set up a Google OAuth client](/how-to/set-up-a-google-oauth-client) names every console screen.

### MCP servers

The open Model Context Protocol standard. Third-party processes, local (stdio via `npx` or `uvx`) or remote (HTTP/SSE), which expose tools consumable by any MCP client. In the **Add a connection** sheet, the **Catalogue** tab opens on the connections built into Apollia (a folder of your computer, Google Workspace, Microsoft 365, a mailbox), then on the selection Apollia ships, each entry with its label, its description and an **Official** or **Community** badge. The public MCP registry is read only when you click **Explore all**; it is paginated as you scroll, so its size is whatever the registry holds on the day you look. The selection holds these entries:

Notion, Linear, Atlassian (Jira + Confluence), Stripe, Figma (Dev Mode), Sentry, Cloudflare, PostgreSQL, Time, Fetch, Memory (Knowledge Graph), Brave Search.

You can also add your own server, on the **Custom connection** tab of the same sheet. With developer mode on, the two tabs read **MCP catalogue** and **Custom MCP**. Changing the catalogue itself is not possible: the override file is read by nothing.

## Where to start

- Mail, calendar, personal or work drive: see [Connect Google Workspace](connecter-google-workspace.md) or [Connect Microsoft 365](connecter-microsoft-365.md).
- Notion, Linear, Atlassian, Stripe, etc.: see [Connect an MCP server](connecter-un-serveur-mcp.md).
- Your internal MCP servers: see [Wire your own MCP server](cabler-son-propre-serveur-mcp.md).
- Tailoring the catalogue to your team is not possible.

## How to choose

| Service | Native connector | Official MCP | Recommendation |
|---|---|---|---|
| Gmail, Google Calendar, Drive | Apollia | None | Native connector |
| Outlook, Calendar, OneDrive | Apollia | None | Native connector |
| Notion, Linear | None | Official | Catalogue |
| Atlassian (Jira + Confluence) | None | Atlassian Rovo | Catalogue |
| Stripe, Figma, Sentry, Cloudflare | None | Official | Catalogue |
| Your internal server | None | To be wired | Custom connection |

## Staying in control

- **Your agreement first**: every write (sending mail, creating an event, writing a file) asks for your agreement before execution. See [Understand MCP permissions](understand-mcp-permissions.md).
- **Local tokens**: tokens are kept on your machine and sent only to the service they belong to. See [Manage OAuth tokens](manage-oauth-tokens.md).
- **Organisation policy**: forbidding the cloud for connections is a rule of the signed policy your organisation may deploy, `connections.remote = "deny"`, checked by the runtime. Under it, Apollia refuses to add a connection to an online service, a Google or Microsoft account among them, and says so in the connection dialog. Without a policy you are free: no connection is refused for being online. The policy is read when Apollia starts: a connection it forbids, already in place, is then listed under **Forbidden by the policy** and its tools are refused.

## Verification

- The **Connections** page opens and lists your connections.
- The **Add a connection** button is visible at the top of the page, and opens a sheet with a **Catalogue** tab and a **Custom connection** tab.

## If it does not work

- **The Connections page is empty or does not load**: restart Apollia, the runtime may not have finished initializing the MCP client.
- **Connecting a native connector stops with "Refused by your organisation's policy"**: your organisation's policy denies connections to online services on this computer. Nothing was sent; ask the person who manages the policy for an exception.

> **Technical reference:** [Apollia reference](/reference) , Tool Registry architecture, scoping, tool governance.
