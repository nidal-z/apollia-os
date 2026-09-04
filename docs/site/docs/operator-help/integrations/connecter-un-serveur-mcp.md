---
title: Connect an MCP server from the catalogue
slug: /operator-help/integrations/connect-an-mcp-server
sidebar_position: 5
---

# Connect an MCP server from the catalogue

> For any operator who wants to enable an MCP server from the catalogue (Notion, GitHub, Linear, Atlassian, Stripe, Time, etc.) in a few clicks.

## Prerequisites

- Apollia running, the **Connections** page reachable.
- You know which service you want to plug in. The catalogue is the public MCP registry; eighteen of its entries carry an Apollia presentation layer, and those are the ones listed in [Integrations overview](vue-d-ensemble-integrations.md).
- For authenticated services, your credentials (API key or OAuth account with the provider).

## Steps

1. In the sidebar, open **Connections**, then click **Add a connector** at the bottom of the connector sidebar. A sheet opens on its **MCP catalogue** tab.

   ![Connections page: the catalogue open on the Discover tab, with its grid of entries](/img/operator-help/integration-connecter-un-serveur-mcp-1.png)

2. Filter or search for the entry you want, then click it. The 4-step wizard starts.

### Step 1, Disclaimer

Four checkboxes recall the implications of installing an external MCP (third-party code runs on your machine, data may be transferred, you can revoke at any time, capabilities are visible before install). Tick all four, then click **Next**.

On screen: step 1 of the wizard, with the 4 checkboxes and their labels, and the Next button greyed out until everything is ticked.

### Step 2, Authentication

Apollia automatically detects the authentication type required by the server. Three possible cases:

- **No authentication**: message *"No authentication required"*. Click **Next**.
- **API key or static token**: a password field appears. Paste your key.
- **OAuth**: a *"Sign in to [Provider]"* button appears with the list of requested scopes. Click it, your browser opens the consent page, authorize, the return is automatic.

On screen: step 2 of the wizard in the OAuth case, with the Sign in to [Provider] button and the list of requested scopes below it.

### Step 3, Test

Click **Test connection**. During the test, the icon pulses. At the end, a badge shows the result:

- **Green**: *"X tools discovered"*. The server responds.
- **Red**: a precise error message (invalid key, unreachable URL, etc.).

If the test fails, go back to step 2 to correct it.

On screen: step 3 of the wizard, with the Test connection button and a green badge showing 12 tools discovered.

### Step 4, Coaching

Apollia shows a few example cards with a *"Try"* button that pre-fills the chat box. Click **Finish** to close the wizard.

On screen: step 4 of the wizard, with 3 example cards each carrying a Try button, and the Finish button.

## Verification

- The server appears in the **Connections** sidebar with a green dot.
- The detail panel shows the tools declared by the server, with their description.
- In free chat, run a prompt suggested by the Coaching step. The matching tool is called.

> **Note - deferred loading:** by default, `[mcp] tool_loading = "deferred"`. The server's tool schemas are not all put in front of the model at startup: below `tool_search_limit` the whole index is declared, above it the assistant calls `tool_search` to find what it needs. The tool count shown in the UI stays complete either way. See [Configure deferred loading of MCP tools](configure-deferred-mcp-loading.md).

## If it does not work

- **The test fails with "Authentication refused"**: your key or token is invalid or revoked. Go back to step 2 and paste the value again without stray spaces.
- **The test fails with "Service not found"**: the server is not reachable. Check your connection or the provider's status.
- **The installed server exposes no tool**: the server starts but declares nothing. See [Test an MCP connection](test-an-mcp-connection.md) to run the test again, then check the logs on the provider side.
- **You want to plug in a server that is not in the catalogue**: see [Wire your own MCP server](cabler-son-propre-serveur-mcp.md).
- **The agent says it has no access to the tool in deferred mode**: `tool_search` is injected by the runtime into the built-in conversational assistant only, and no manifest key declares it. An installed agent reaches an MCP tool by naming it in `tools_required` or `tools_optional`, in the `mcp:<server>/<tool>` form. Update the manifest, or raise `tool_search_limit` so the whole index is declared up front.
- **The agent says it has no access to the tool**: open the agent's page, the Tools tab lists what its manifest declares. If the tool is not there, the agent is what needs updating. See [Understand the scope of an integration](understand-integration-scope.md).

> **Technical reference:** [Apollia reference](/reference) , MCP protocol, transports, trust levels, governance.
