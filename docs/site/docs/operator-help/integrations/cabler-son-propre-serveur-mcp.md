---
title: Wire your own MCP server
slug: /operator-help/integrations/wire-your-own-mcp-server
sidebar_position: 9
---

# Wire your own MCP server

> For any operator or builder who wants to connect an MCP server that is not in the catalogue, by its address, or as a local command with developer mode on.

## Prerequisites

- Apollia running.
- An MCP server that conforms to the spec: reached at an address (**streamable-http**, or **sse** with developer mode on), or launched as a local command (**stdio**, with developer mode on).
- What the server needs: its address and, when it asks one, its key; or the command and its arguments.

## Steps

1. In **Connections**, click **Add a connection**. In the window, choose **Connect a service by its address**, always last in the list whatever you search, then click **Add**.
2. Fill in the form, as described below.
3. Click **Test**. Apollia attempts a real connection and counts the tools declared by the server.
4. Click **Install**. The addition continues as for any connection: **Who can use it**, where you check the agents that get its tools, then **Try it**. See [Connect an MCP server from the catalogue](connecter-un-serveur-mcp.md).

On screen: the form of a service by its address, with its Name and Address fields and the Test and Install buttons.

### A server reached at its address

- **Name (internal, unique)**: optional. Left empty, the server is named after the host of its address. What you type is rewritten: lowercased, and every run of characters outside `a-z`, `0-9`, `_` and `-` replaced by a single hyphen, leading and trailing hyphens dropped. So `My Server!` is installed as `my-server`; check the name the list shows afterwards.
- **Address**: the HTTP endpoint of the server (`https://...`).
- **Access key**: the field appears when the test finds that the server asks for a key, or when you tick **The service gave me a key**. The key is sent to the server as its authorization, and kept in the secret store of this computer, never in the settings of the connection.

### With developer mode on

A **Transport** choice appears, with three values: **Streamable HTTP**, **SSE**, and **stdio (local subprocess)**.

- **stdio**: **Command**, the executable to launch (for example `npx`, `uvx`, or an absolute path); **Arguments**, separated by spaces (for example `-y @modelcontextprotocol/server-filesystem ~/Documents`); **Environment variables**, one per line as `NAME=value`.
- **Streamable HTTP** and **SSE**: the **Address**, and **HTTP headers**, one per line as `NAME=value`, for example `X-API-Key=...`.

### Where the secrets go

A value you type as a header, and a value of a variable whose name marks it secret (it contains `KEY`, `TOKEN`, `SECRET`, `PASSWORD`, `AUTH` or a similar word), goes to the secret store of this computer when the server is added or its settings are saved. The settings of the connection keep only a reference to it. A server added with an earlier version, whose values were kept in its settings, has them moved to the secret store at the first launch of this version.

## OAuth 2.1, from the catalogue only

When a server of the catalogue advertises an OAuth endpoint that conforms to the MCP authorization spec (RFC 9728 Protected Resource Metadata + RFC 8414 Authorization Server Metadata), the window that adds it handles the sign-in on its own:

1. Metadata discovery (PRM, then OIDC fallback).
2. Client identification through the Apollia CIMD, or Dynamic Client Registration (RFC 7591) as a fallback.
3. Code exchange with PKCE S256 and Resource Indicators (RFC 8707).
4. Token storage in the local keychain and proactive refresh with singleflight.

The form of a service by its address does not run this flow. When its **Test** meets a server that requires OAuth, it answers "This server requires OAuth authentication. Install it from the catalogue rather than via the custom form." A server that accepts a static key is wired here with **Access key**.

## Local mDNS discovery

Apollia can scan your local network for MCP servers that advertise themselves through mDNS (service type `_apollia-mcp._tcp.local.`). No option of the interface runs that scan: it is a command, `apollia-os mcp list --discover`, which lists the configured servers and adds the ones it found on the network.

## Verification

- The server is in the list of connections, under **Services**.
- Its card lists the tools the server advertises, each with its family.
- **Check** on its card reports it **Working** (see [Check a connection and repair it](test-an-mcp-connection.md)).

## Security, what Apollia applies by default

- **Presentation**: a server added this way appears in the list without the logo and the description that the entries of Apollia's selection carry.
- **Approval**: the server carries no approval setting of its own. Each tool it publishes is decided on its action family: what the server declares in the tool's annotations sorts a read from a write, and a tool it does not describe is treated as sensitive. On the card, **Change** gives a tool another family when you know better; the change is written in the **Activity log**. An agent that calls a sensitive tool asks first, and you answer once, always for this agent, or refuse.
- **Roots**: Apollia advertises the capability during the handshake, and answers no `roots/list` request. No directory is declared, so roots bounds nothing today. What bounds a stdio server is the command and the arguments you gave it, which is why the filesystem path you pass in **Arguments** is the real perimeter.
- **Sampling and elicitation**: not implemented. Apollia does not advertise these two capabilities during the handshake, so a server that supports them will not try to call back through them.

## When the server runs

The server is not kept running. It starts at the first call of one of its tools and stops after 15 minutes without one, by default; its tools stay listed in between. How its tools are put in front of the agent, all at once or found through `tool_search`, is a setting of its own. See [Configure deferred loading of MCP tools](configure-deferred-mcp-loading.md).

## If it does not work

- **"Command not found" on stdio**: your binary is not in Apollia's PATH. Give the absolute path or adjust your PATH before launching Apollia.
- **"Connection refused" on HTTP or SSE**: wrong URL or port, or a firewall blocking outbound traffic. Check that the server is reachable from your machine with `curl <url>`.
- **OAuth loop**: your server advertises its metadata endpoint incorrectly, or a scope was refused. Apollia rejects non-conformant authorization servers fail-fast (PKCE S256 is mandatory). Check the server-side logs.
- **"No tool detected"**: the handshake succeeds but the server does not declare the `tools` capability in its `InitializeResult`. Check the server-side implementation.

## Getting your server to show up in the catalogue

Your server appears in the list once connected, but without the logo and the description that the entries of Apollia's selection carry. Customizing the catalogue is not possible from the interface.

> **Technical reference:** [Apollia reference](/reference) , full protocol schema, capabilities, transports, security.
