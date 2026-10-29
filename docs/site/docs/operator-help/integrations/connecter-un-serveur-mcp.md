---
title: Connect an MCP server from the catalogue
slug: /operator-help/integrations/connect-an-mcp-server
sidebar_position: 5
---

# Connect an MCP server from the catalogue

> For any operator who wants to add a service from the catalogue (Notion, Linear, Atlassian, Stripe, Time, etc.) in a few clicks, and choose which agents may use it.

## Prerequisites

- Apollia running, the **Connections** page reachable.
- You know which service you want to plug in. The selection Apollia offers is listed in [Integrations overview](vue-d-ensemble-integrations.md); the public catalogue is one click further.
- For services that ask for one, your key or your account with the provider.

## Find the service

1. In the sidebar, open **Connections**, then click **Add a connection**. A window opens on a single search.
2. Type the name of the service. The answers come in four groups:
   - **Offered by Apollia**: the Microsoft 365 and Google Workspace accounts, then the services Apollia selected, each marked **Official** or **Community**.
   - **On this computer**: a folder, a mailbox.
   - **Public catalogue**: asked only when you click **Explore all**. Anyone can publish there and Apollia has not checked its entries, so each one is marked **Not verified**.
   - **Connect a service by its address**: always last, for a service you reach by its address. See [Wire your own MCP server](cabler-son-propre-serveur-mcp.md).
3. Click the service. Its card says **What it asks**: a sign-in in your browser, a key, a few settings, a download to this computer, or that the service is reached over the internet. Click **Add**. A service already added offers **Open its card** instead.

## Add it in three steps

The steps show at the top of the window: **Connect**, **Who can use it**, **Try it**. **Cancel** before the end of the first step brings you back to the search, and nothing is added.

### Connect

For a service of the catalogue, its card already showed **Before adding this connection**, above **Add**: your agents will be able to use what the service offers, an action that sends, changes or deletes something asks for your agreement first, what they read or send goes through the service under its own terms, and you can remove the connection at any time. Clicking **Add** gave your agreement, and it is recorded in the **Activity log**.

Then Apollia asks only what the service needs:

- **Nothing**: the service is added straight away.
- **A key**: paste it in the field. It is kept in the secret store of this computer, never in the settings of the connection.
- **A sign-in**: click the sign-in button, your browser opens the consent page of the provider, authorize, and the return is automatic. When the service publishes its permissions, **What the sign-in will grant** lists them before you sign in. Choosing which ones to grant is for developer mode only.

A component the service needs is downloaded first when required, and its integrity is checked before it is installed.

### Who can use it

**Which agents can use this connection?** **Apollia, in the Chat** comes first and checked; the other agents of this computer are listed to check. The agents checked get every tool of the connection. A tool that writes, sends or runs code, and one nobody classified, still asks for your approval before it runs. Click **Continue**.

You can change this later, on the card of the connection with **Give to an agent**, or on the sheet of each agent.

### Try it

Apollia tries the connection at once and shows the result:

- **Connection added and working**, with the number of tools available.
- **The connection is added, but its trial failed**, with a clear message. The connection stays added: **Retry** tries again, **Remove** takes it out.

A refusal says who refused, your organisation's policy for instance.

Under the result, **What can you do now?** offers a few examples, each with a **Try** button that opens a conversation with Apollia limited to the tools of this connection.

## Verification

- The connection is in the **Connections** list. Its state reads **Working**, or **Not checked yet** until it is first used or checked.
- Its card lists the tools of the service, each with its family, and under **Agents that use it**, the agents you checked.
- Click **Try** on the card and ask something the service can do. The matching tool is called.

> **Note:** a service starts at its first use and stops after a while without one. Its tools stay listed in between. See [Configure deferred loading of MCP tools](configure-deferred-mcp-loading.md).

## If it does not work

- **The trial fails with "Authentication refused"**: the key is invalid or revoked. Click **Remove**, then add the service again and paste the key without stray spaces.
- **The trial fails with "Service not found"**: the service answered that nothing is at the address it was given. Check the address or the name of the service.
- **The service exposes no tool**: it starts but declares nothing. Run [a check](test-an-mcp-connection.md), then look at the logs on the provider side.
- **The service you want is not in the catalogue**: see [Wire your own MCP server](cabler-son-propre-serveur-mcp.md).
- **An agent says it has no access to the tool**: it was not checked under **Who can use it**. On the card of the connection, click **Give to an agent**. See [Understand the scope of an integration](understand-integration-scope.md).

### A service asks for Node.js {#nodejs}

Some services (the web browser, the files of a folder, and others) run on Node.js. Apollia downloads its own the first time one of them is added. When it cannot (no network, or its download is not available yet), it uses the Node.js already installed on your computer, if its version is 22 or later.

- **The addition says "This service needs Node.js"**: no usable Node.js was found. Install Node.js 22 or later from [nodejs.org](https://nodejs.org) (the LTS release), then add the service again. You can also wait until Apollia can download its own.
- **The message names a Node.js "older than the 22 release"**: the one installed on your computer is too old. Update it to version 22 or later.

> **Technical reference:** [Apollia reference](/reference) , MCP protocol, transports, trust levels, governance.
