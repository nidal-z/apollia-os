---
title: Test an MCP connection
slug: /operator-help/integrations/test-an-mcp-connection
sidebar_position: 6
---

# Test an MCP connection

> For any operator who wants to check that an installed MCP server really responds, or diagnose a red light.

## Prerequisites

- At least one MCP server installed (see [Connect an MCP server](connecter-un-serveur-mcp.md)).

## Steps

1. In the **Connections** sidebar, select the MCP server to test.

   ![Connections page: an MCP server selected in the sidebar, its detail page on the right](/img/operator-help/integration-tester-une-connexion-mcp-1.png)

2. In the detail panel header, click **Test**. It is a plain button next to **Reconnect**, with a refresh icon; there is no plug icon and no actions menu.

   ![Page of an installed MCP server, with the Test button in the actions area](/img/operator-help/integration-tester-une-connexion-mcp-2.png)

3. During the test, the button shows a spinner and is disabled. The test typically takes less than a second.

4. The result appears as one line under the tabs. It carries a tool count, never a latency: nothing on the MCP path measures a response time.

   - **Working - N tools, last operation succeeded**, in green.
   - **Reachable - N tools listed, not yet verified by an operation**, in grey. The server answered the handshake, but no operation has confirmed it since.
   - **Reachable, but recent operations failed** or **Reachable, but authorization expired**, in amber.
   - The raw error message, in red, when the call itself failed.

   On screen: the green Working line shown under the tabs of the server detail.

## Translated error messages, in the installation wizard

These messages belong to the wizard that installs a server, not to the test of an already-installed one. On the detail panel above, a failed test shows the backend message as it came, with no translation and no **Show technical details** link. In the wizard, Apollia turns technical errors into clear messages:

- *"Authentication refused - check your API key."*: invalid or expired token (HTTP 401).
- *"Access forbidden - your key doesn't have the required rights."*: insufficient permissions (HTTP 403).
- *"Service not found - check the URL or server name."*: wrong URL or missing server (HTTP 404).
- *"Network error - the service did not respond in time."*: timeout or connection failure.
- *"Command not found - the package is probably not installed."*: stdio transport, missing binary.
- *"The connection failed. Check your credentials and try again."*: generic error.

In the wizard, a **Show technical details** disclosure reveals the raw backend message, and a **Fix authentication** button sends you back to the credentials step.

## Verification

- The result line reads **Working** or **Reachable**, and its tool count is non-zero.
- The expected tools appear in the **Tools** tab.
- If some are missing, look at Apollia first: discovery keeps at most `max_tools` tools per server, 256 by default, and logs `mcp.tools.bounded` with what it kept and what it received when it cuts. A server response is also capped at `max_response_bytes`. Only once both are ruled out does the remote server become the explanation.

## If it does not work

- **Network error**: your machine cannot reach the server. Check your internet connection or, for a stdio MCP, the PATH of the command.
- **Authentication refused**: use **Reconnect** in the detail header, and if that is not enough, disconnect the server and install it again with valid credentials.
- **Access forbidden**: your account does not have the rights on the provider side. Check the granted scopes or raise the permissions in the business tool.
- **Command not found (stdio)**: for an MCP on stdio transport, the binary is not in Apollia's PATH. Install the tool or adjust the command.
- **Test OK but the agent does not call the tool**: open the agent's page, check that this MCP is in its manifest. See [Understand the scope of an integration](understand-integration-scope.md).

> **Technical reference:** [Apollia reference](/reference) , full error codes, MCP handshake semantics.
