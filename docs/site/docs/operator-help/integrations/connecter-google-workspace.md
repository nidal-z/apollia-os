---
title: Connect Google Workspace
slug: /operator-help/integrations/connect-google-workspace
sidebar_position: 3
---

# Connect Google Workspace

> For any operator who wants to plug Gmail, Calendar, Drive, Sheets, Docs, Slides, Forms or Tasks into Apollia.

A personal `@gmail.com` account works. Nothing here needs a Workspace subscription, a company domain, or an administrator.

## Prerequisites

- Apollia running.
- A Google account, personal or Workspace.
- **Your own OAuth client**, set up once, see the section right below.
- Your organisation's policy, if it has deployed one, does not deny remote connections. Otherwise the connection is refused when you click.
- An active internet connection.

:::info Google asks more of you than Microsoft does
Microsoft 365 connects straight away, with nothing to register. Google does not, and cannot: about ten minutes in the Google Cloud console come first. The reason is Google's, not Apollia's, and it is spelled out below.
:::

## Set up your OAuth client, once

<!-- claim:oauth-google-client-not-embedded -->
Apollia ships without a Google OAuth client, and no published build embeds one. You register your own application with Google and hand its credentials to Apollia. Budget ten minutes the first time.

If you want each console screen named click by click, follow [Set up a Google OAuth client](/how-to/set-up-a-google-oauth-client) and come back here at the "In Apollia" step. The short version follows.

**Why there is no shared client.** Google will not let an application serve accounts outside its own project until its consent screen has passed verification, and the scopes classified *restricted* (`gmail.readonly`, `gmail.modify`, `gmail.compose`, `drive.readonly`, `drive`) additionally require a CASA Tier 2 audit by a Google-approved third party, billed 5,000 to 15,000 dollars a year. A shared Apollia application would also put every user behind one quota and one consent screen. Your own client keeps you in control of both. Microsoft has no equivalent requirement for a public desktop client, which is the whole of the difference between the two pages.

**What Testing status costs you.** Leaving the consent screen in **Testing** is free and immediate, and it has two limits worth knowing before you start: at most 100 test users, and **refresh tokens expire after seven days**, so you will be asked to reconnect the account roughly once a week. Moving the screen to **Production** without verification removes the seven-day expiry but shows an "unverified app" warning that you have to click through. Verification itself is free for the scopes Apollia requests by default, and takes several weeks.

**In the Google Cloud console.**

1. Create a project, then enable the APIs the consent step will ask for: Gmail, Calendar, Drive, Sheets, Docs, Slides, Tasks and Forms. Each lives behind its own API in the console, and a service whose API is off fails at call time even though the scope was granted.
2. Configure the OAuth consent screen in **External** mode, leave it in **Testing** status, and add your own address as a test user.
3. Create an OAuth client of type **Desktop app**.
4. **Download the JSON file** the console offers. Keep it, you need it in the next step.

<!-- claim:oauth-google-client-json-import -->
**In Apollia.**

1. Open **Connections**, click **Add a connection**, and choose **Google Workspace** in the catalogue. Its page opens on the **Accounts** tab, which carries the **Google client** section.
2. Click **Import the client JSON** and pick the file you just downloaded. The client ID and the client secret are read from it. The client ID is kept in a configuration file of your computer; the client secret is kept in the system keychain, never in a file.
3. Click **Test the client**. It should answer **Configuration looks valid.**

If you prefer to type the values yourself, the **Client ID** and **Client secret** fields of the same section accept them directly.

**Why a client secret.** Google issues a `client_secret` alongside the client ID for a Desktop client, and requires it when the authorization code is exchanged for a token, even though Apollia also uses PKCE. Apollia keeps it in the system keychain and never sends it anywhere but Google.

<!-- claim:oauth-connect-refuses-before-consent -->
If either half is missing, Apollia refuses the connection before opening your browser and tells you which one, rather than sending you through a consent screen that cannot complete.

**Alternative for a shell or a headless host.** `APOLLIA_GOOGLE_CLIENT_ID` and `APOLLIA_GOOGLE_CLIENT_SECRET` take priority over what was entered on the page. They only apply to processes launched from the shell where you exported them, which is a common reason a client looks configured but is not. See [Environment variables](/reference/environment-variables).

## Steps

1. In the sidebar, open **Connections**, then open the **Google Workspace** page: from the list, or from **Add a connection** when it is not listed yet.

   ![Google Workspace page in Connections, not connected, with the Accounts tab and the Connect an account button](/img/operator-help/integration-google-workspace-1.png)

2. Click **Connect an account**. A window opens inside Apollia and your browser automatically opens the Google consent page.

3. Pick the Google account to use, then accept the permissions offered (Mail, Calendar, Drive Workspace, and so on).

   ![Google consent screen, Apollia asks for access to the account, list of permissions (app Drive files, Calendar events, sending mail, draft management), warning that the app is not verified by Google](/img/operator-help/integration-google-workspace-2.png)

4. Back in Apollia, the window detects the return automatically. A second step offers you the agent Drive root folder (default **Apollia**). Confirm by clicking **Save** (or **Keep the default** to keep the value offered).

   ![Google Drive folder dialog in Apollia, explanation of the drive.file scope, Folder path field with the value Apollia, Keep the default and Save buttons](/img/operator-help/integration-google-workspace-3.png)

5. The window confirms that Google Workspace is added, with **Try** and **Done**. Your account appears under the **Accounts** tab of the page.

   On screen: the Accounts tab of the Google Workspace page, showing the connected account and its Disconnect button.

## What you can do

**Reads**: list your Calendar events, browse the `Apollia/` folder on Drive, read Sheets cells, read Docs text, list your tasks. Reading your account is reading personal data, so Apollia asks for your agreement; answering **Always for this agent** stops the question for that agent.

Two YouTube operations exist in the catalogue and cannot be used: the consent step never asks for the YouTube scope, so neither of them can authenticate. Treat YouTube as absent.

**Writes and sends (with your agreement)**: send a mail, create a draft, create or change a Calendar event, write a Drive Workspace file, add or change values in Sheets, append text to a Doc, create a Slide, create a form, create or complete a task.

**Deletions (with your agreement, like any write)**: delete a Calendar event, delete a task. There is no phrase to type.

## Workspace Drive pattern

With the `drive.file` scope, the application only sees the files it created or the ones you explicitly open for it. Apollia builds on that behaviour:

- The `Apollia` root folder is created on the first write, not at connection time. The step where you name it only records the path.
- Every time an agent creates a file, an `Apollia/<agent-name>/` subfolder is created on demand.
- Drive operations are scoped to that folder. The agent does not see the rest of your Drive.

So the agent can save a `meeting-notes.md` note in its workspace, read it back later, and delete it, without ever seeing the rest of your Drive.

## Multiple accounts

You can connect several Google accounts, and each appears under the **Accounts** tab with its email address. Each connected account is a connection of its own. Its tools are granted to an agent account by account, on the agent's sheet, and a call runs as the account that was granted and as no other.

## Verification

- The **Accounts** tab lists the account, and the page header reads **1 active**.
- On the page, click **Try** and ask for example *"List my last 3 Calendar events"*. Apollia answers with the tools of this connection, and asks for your agreement before it reads the account.
- Then try *"Send a mail to <your address> with the subject test"*. A consent request appears before sending.
- Your system keychain (Keychain on macOS) holds an `apollia-connector-google` entry tied to your email address.

## If it does not work

- **The Google Workspace page reads "Setup required"**: no OAuth client is configured yet. Click **Set up credentials** and follow the section at the top of this page.
- **Apollia says the client secret is missing**: the client ID was saved but not its secret. Re-import the JSON file from the Google Cloud console, which carries both, or paste the secret into the **Client secret** field of the **Google client** section, on the Google Workspace page.
- **The Google consent screen shows "This app is not verified"**: expected, since the application is yours and sits in Testing status. Click **Advanced** then **Go to Apollia** to continue.
- **Connecting stops with "Refused by your organisation's policy"**: your organisation's policy denies connections to online services on this computer, and nothing was sent to Google. Only the person who manages that policy can grant an exception; nothing in Apollia lifts it.
- **You want full Gmail or Drive read access**: those scopes are restricted (Google CASA audit). No Apollia tool uses them yet, see "About the restricted scopes" below.
- **The agent does not see a specific file on Drive**: it only has access to the `Apollia/<agent>/` folder. Drop the file into that folder or pass it the explicit identifier in your prompt.
- **An agent asks for a full-read Gmail tool**: there is none. The Google operation catalogue only contains non-restricted scopes, and a test locks that down. The agent will get an unknown tool error, not a scope error.

## About the restricted scopes

Your OAuth consent screen can offer the *restricted* scopes (`gmail.readonly`, `gmail.modify`, `gmail.compose`, `drive.readonly`, `drive`), but **no Apollia tool uses them yet**: the Google operation catalogue contains none of them, and a test locks that down. Granting one unlocks no capability for now. The default perimeter already covers sending, composing, the full calendar and the scoped Drive.

**Responsibility.** The OAuth application is yours and Apollia does not audit that configuration. If you distribute a build with your client ID embedded beyond 100 users, Google will require the CASA Tier 2 audit.

**Alternative.** If the Google Cloud console feels heavy, a community Gmail MCP server (search for `mcp-server-gmail`) runs locally with your credentials and exposes the Gmail tools through MCP. See [Wire your own MCP server](cabler-son-propre-serveur-mcp.md).

## Disconnecting an account

On the Google Workspace page, under the **Accounts** tab, click **Disconnect** next to the account concerned and confirm. The token is deleted from the local secret store. To revoke on the Google side too, go to https://myaccount.google.com/permissions and remove the Apollia application.

> **Technical reference:** [Apollia reference](/reference) , full scopes, proactive refresh, multiple accounts, keychain storage.
