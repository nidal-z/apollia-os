# Connect Google Workspace

> For any operator who wants to plug Gmail, Calendar, Drive, Sheets, Docs, Slides, Forms, Tasks or YouTube into Apollia.

A personal `@gmail.com` account works. Nothing here needs a Workspace subscription, a company domain, or an administrator.

## Prerequisites

- Apollia running.
- A Google account, personal or Workspace.
- **Your own OAuth client**, set up once, see the section right below.
- Your sovereignty profile is not set to `local_only` (otherwise the cloud buttons are greyed out).
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

1. Create a project, then enable the Gmail, Calendar and Drive APIs.
2. Configure the OAuth consent screen in **External** mode, leave it in **Testing** status, and add your own address as a test user.
3. Create an OAuth client of type **Desktop app**.
4. **Download the JSON file** the console offers. Keep it, you need it in the next step.

<!-- claim:oauth-google-client-json-import -->
**In Apollia.**

1. Open **Settings → OAuth integrations**.
2. On the Google card, click **Import JSON** and pick the file you just downloaded. The client ID and the client secret are read from it and stored in `~/.apollia/oauth-clients.toml`, readable by your user only.
3. Click **Test configuration**. It should report that the client is present, well-formed, and that Google's authorization server is reachable.

If you prefer to type the values yourself, the two fields on the same card accept them directly.

**Why a client secret.** Google issues a `client_secret` alongside the client ID for a Desktop client, and requires it when the authorization code is exchanged for a token, even though Apollia also uses PKCE. Apollia stores it locally and never sends it anywhere but Google.

<!-- claim:oauth-connect-refuses-before-consent -->
If either half is missing, Apollia refuses the connection before opening your browser and tells you which one, rather than sending you through a consent screen that cannot complete.

**Alternative for a shell or a headless host.** `APOLLIA_GOOGLE_CLIENT_ID` and `APOLLIA_GOOGLE_CLIENT_SECRET` take priority over the file. They only apply to processes launched from the shell where you exported them, which is a common reason a client looks configured but is not. See [Environment variables](/reference/environment-variables).

## Steps

1. In the sidebar, open **Connections**, then select the **Google Workspace** card in the list of native connectors.

   ![Connections page, Google Workspace card selected in the sidebar (Not connected state), right-hand panel with the Accounts (0) tab and the Connect an account button](/img/operator-help/en/integration-google-workspace-1.png)

2. Click **Connect an account**. A window opens inside Apollia and your browser automatically opens the Google consent page.

3. Pick the Google account to use, then accept the permissions offered (Mail, Calendar, Drive Workspace, and so on).

   ![Google consent screen, Apollia asks for access to the account, list of permissions (app Drive files, Calendar events, sending mail, draft management), warning that the app is not verified by Google](/img/operator-help/en/integration-google-workspace-2.png)

4. Back in Apollia, the window detects the return automatically. A second step offers you the agent Drive root folder (default **Apollia**). Confirm by clicking **Save** (or **Keep the default** to keep the value offered).

   ![Google Drive folder dialog in Apollia, explanation of the drive.file scope, Folder path field with the value Apollia, Keep the default and Save buttons](/img/operator-help/en/integration-google-workspace-3.png)

5. The window closes, your account appears in the sidebar with a green dot.

   On screen: the Connections sidebar, with the Google Workspace card expanded showing the connected account, its green dot and the Disconnect button.

## What you can do

**Reads (without approval)**: list your Calendar events, browse the `Apollia/` folder on Drive, read Sheets cells, read Docs text, list your tasks, search YouTube videos.

**Writes (with HITL approval)**: send a mail, create a draft, create or change a Calendar event, write a Drive Workspace file, add or change values in Sheets, append text to a Doc, create a Slide, create a form, create or complete a task.

**Deletions (with a confirmation phrase)**: delete a Calendar event, delete a task.

## Workspace Drive pattern

With the `drive.file` scope, the application only sees the files it created or the ones you explicitly open for it. Apollia builds on that behaviour:

- On the first connection, an `Apollia` root folder is created at the root of your Drive.
- Every time an agent creates a file, an `Apollia/<agent-name>/` subfolder is created on demand.
- Drive operations are scoped to that folder. The agent does not see the rest of your Drive.

So the agent can save a `meeting-notes.md` note in its workspace, read it back later, and delete it, without ever seeing the rest of your Drive.

## Multiple accounts

You can connect several Google accounts. Each account appears in the sidebar with its email address. When an agent calls a Google tool, it can pick the target account through an `account` parameter if several accounts are connected.

## Verification

- The dot next to the account is green.
- In free chat, ask for example *"List my last 3 Calendar events"*. The answer comes back without an approval request.
- Then try *"Send a mail to <your address> with the subject test"*. An approval popup appears before sending.
- Your system keychain (Keychain on macOS) holds an `apollia-connector-google` entry tied to your email address.

## If it does not work

- **The Google Workspace card reads "Setup required"**: no OAuth client is configured yet. Click **Set up credentials** and follow the section at the top of this page.
- **Apollia says the client secret is missing**: the client ID was saved but not its secret. Re-import the JSON file from the Google Cloud console, which carries both, or paste the secret into the Google card in **Settings → OAuth integrations**.
- **The Google consent screen shows "This app is not verified"**: expected, since the application is yours and sits in Testing status. Click **Advanced** then **Go to Apollia** to continue.
- **The Connect button is greyed out**: your sovereignty profile is `local_only`. Cloud connectors are disabled in that mode.
- **You want full Gmail or Drive read access**: those scopes are restricted (Google CASA audit) and out of scope for v0.1.0. No Apollia tool uses them yet, see "About the restricted scopes" below.
- **The agent does not see a specific file on Drive**: it only has access to the `Apollia/<agent>/` folder. Drop the file into that folder or pass it the explicit identifier in your prompt.
- **An agent asks for a full-read Gmail tool**: there is none. The Google operation catalogue only contains non-restricted scopes, and a test locks that down. The agent will get an unknown tool error, not a scope error.

## About the restricted scopes

Your OAuth consent screen can offer the *restricted* scopes (`gmail.readonly`, `gmail.modify`, `drive.readonly`, `drive`), but **no Apollia tool uses them yet**: the Google operation catalogue contains none of them, and a test locks that down. Granting one unlocks no capability for now. The default perimeter already covers sending, composing, the full calendar and the scoped Drive.

**Responsibility.** The OAuth application is yours and Apollia does not audit that configuration. If you distribute a build with your client ID embedded beyond 100 users, Google will require the CASA Tier 2 audit.

**Alternative.** If the Google Cloud console feels heavy, a community Gmail MCP server (search for `mcp-server-gmail`) runs locally with your credentials and exposes the Gmail tools through MCP. See [Wire your own MCP server](cabler-son-propre-serveur-mcp.md).

## Disconnecting an account

On the Google Workspace card, click **Disconnect** next to the account concerned. The token is immediately deleted from the local keychain. To revoke on the Google side too, go to https://myaccount.google.com/permissions and remove the Apollia application.

> **Technical reference:** [Apollia reference](/reference) , full scopes, proactive refresh, multiple accounts, keychain storage.
