# Test an MCP connection

> For any operator who wants to check that an installed MCP server really responds, or diagnose a red light.

## Prerequisites

- At least one MCP server installed (see [Connect an MCP server](connecter-un-serveur-mcp.md)).

## Steps

1. In the **Connections** sidebar, select the MCP server to test.

   ![Connections page: an MCP server selected in the sidebar, its detail page on the right](/img/operator-help/en/integration-tester-une-connexion-mcp-1.png)

2. In the detail panel, click the **plug** icon next to the server name, or **Test connection** in the actions menu.

   ![Page of an installed MCP server, with the Test button in the actions area](/img/operator-help/en/integration-tester-une-connexion-mcp-2.png)

3. During the test, the icon pulses and the button is disabled. The test typically takes less than a second.

4. The result appears as a badge:

   - **Green**: *"OK · XXX ms"*. The server responds, the latency is shown.
   - **Red**: *"Error: <translated message>"*. The server does not respond, the message states the cause.

   On screen: the green OK · 247 ms badge shown under the test button.

## Translated error messages

Apollia turns technical errors into clear messages:

- *"Authentication refused - check your API key."*: invalid or expired token (HTTP 401).
- *"Access forbidden - your key doesn't have the required rights."*: insufficient permissions (HTTP 403).
- *"Service not found - check the URL or server name."*: wrong URL or missing server (HTTP 404).
- *"Network error - the service did not respond in time."*: timeout or connection failure.
- *"Command not found - the package is probably not installed."*: stdio transport, missing binary.
- *"The connection failed. Check your credentials and try again."*: generic error.

A **Show technical details** link displays the raw backend message for advanced users.

## Verification

- Latency below 1 second for a healthy server.
- The tool counter in the detail panel is non-zero.
- The expected tools appear in the list. If some are missing, the remote server may have disabled features on the account side.

## If it does not work

- **Network error**: your machine cannot reach the server. Check your internet connection or, for a stdio MCP, the PATH of the command.
- **Authentication refused**: disconnect the MCP then reconnect with valid credentials, or use the **Fix authentication** button to update the token without reinstalling everything.
- **Access forbidden**: your account does not have the rights on the provider side. Check the granted scopes or raise the permissions in the business tool.
- **Command not found (stdio)**: for an MCP on stdio transport, the binary is not in Apollia's PATH. Install the tool or adjust the command.
- **Test OK but the agent does not call the tool**: open the agent's page, check that this MCP is in its manifest. See [Understand the scope of an integration](comprendre-la-portee-d-une-integration.md).

> **Technical reference:** [Apollia reference](/reference) , full error codes, MCP handshake semantics.
