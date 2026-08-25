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

Maintained directly by Apollia for services that do not (yet) expose an official MCP server: **Google Workspace** (Gmail, Calendar, Drive, Sheets, Docs, Slides, Forms, Tasks, YouTube) and **Microsoft 365** (Outlook, Calendar, OneDrive).

- Tokens stored in the system keyring (macOS Keychain, Windows Credential Manager, Linux Secret Service).
- Direct calls from your machine to `gmail.googleapis.com` or `graph.microsoft.com`, no Apollia cloud relay.
- Automatic HITL approval on every write.
- Multi-account supported.

**The two do not cost the same to start.** Worth knowing before you click, because the difference shows up only once you are on the connector page:

| | What it takes to connect |
|---|---|
| **Microsoft 365** | Nothing. Apollia ships the identifier of its own registered application, so you sign in and you are done. |
| **Google Workspace** | About ten minutes in the Google Cloud console first. You register your own OAuth client and hand its credentials to Apollia. |

This is not an oversight on the Google side. Google requires a verified consent screen before an application may serve accounts outside its own project, and its desktop clients also carry a secret that no distributed binary can hold. Microsoft's public desktop clients require neither. [Connect Google Workspace](connecter-google-workspace.md) explains what the Testing status costs, notably a reconnection about once a week, and [Set up a Google OAuth client](/how-to/set-up-a-google-oauth-client) names every console screen.

### MCP servers

The open Model Context Protocol standard. Third-party processes, local (stdio via `npx` or `uvx`) or remote (HTTP/SSE), which expose tools consumable by any MCP client. Apollia ships a curated catalogue of **18 entries**:

Notion, Slack, GitHub, Linear, Atlassian, Stripe, Figma, Sentry, Cloudflare, PostgreSQL, SQLite, Git, Time, Fetch, Filesystem, Memory, Puppeteer, Brave Search.

You can also add your own servers or modify the catalogue.

![Connections page, left sidebar listing the native connectors (Google Workspace, Microsoft 365) and the MCP servers, right panel with the Overview tab of the selected connector and the Add a connector button at the bottom](/img/operator-help/integration-overview-1.png)

## Where to start

- Mail, calendar, personal or work drive: see [Connect Google Workspace](connecter-google-workspace.md) or [Connect Microsoft 365](connecter-microsoft-365.md).
- Notion, GitHub, Linear, Atlassian, Stripe, etc.: see [Connect an MCP server](connecter-un-serveur-mcp.md).
- Your internal MCP servers: see [Wire your own MCP server](cabler-son-propre-serveur-mcp.md).
- Tailoring the catalogue to your team is not possible in `v0.1.0-preview`.

## How to choose

| Service | Native connector | Official MCP | Recommendation |
|---|---|---|---|
| Gmail, Google Calendar, Drive | Apollia | None | Native connector |
| Outlook, Calendar, OneDrive | Apollia | None | Native connector |
| Notion, Slack, Linear, GitHub | None | Official | Catalogue MCP |
| Atlassian (Jira + Confluence) | None | Atlassian Rovo | Catalogue MCP |
| Stripe, Figma, Sentry, Cloudflare | None | Official | Catalogue MCP |
| Your internal server | None | To be wired | Custom MCP |

## Staying in control

- **HITL approval**: every write (sending mail, creating an event, writing a file) asks for your confirmation before execution. See [Understand MCP permissions](comprendre-les-permissions-mcp.md).
- **Local tokens**: no secret leaves your machine. See [Manage OAuth tokens](gerer-les-tokens-oauth.md).
- **Sovereignty profile**: Apollia accepts cloud connectors by default (`cloud_allowed`). Under the `local_only` profile, the cloud connection buttons are disabled and only purely local stdio MCPs remain available. In v0.1.0, the profile is set on the backend configuration side (no toggle in the interface yet).

## Verification

- The **Connections** page opens and shows the connector sidebar (empty if nothing is plugged in yet).
- The **+ Discover** and **+ Add custom** buttons are visible at the top.

## If it does not work

- **The Connections page is empty or does not load**: restart Apollia, the runtime may not have finished initializing the MCP client.
- **The Connect button of a native connector is greyed out**: your sovereignty profile is `local_only`, see the previous section.
- **You see "Section being redesigned"**: your application predates v0.1.0, update it.

> **Technical reference:** [Apollia reference](/reference) , Tool Registry architecture, scoping, tool governance.
