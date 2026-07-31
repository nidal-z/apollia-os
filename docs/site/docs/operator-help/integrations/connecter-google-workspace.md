# Connect Google Workspace

> For any operator who wants to plug Gmail, Calendar, Drive, Sheets, Docs, Slides, Forms, Tasks or YouTube into Apollia, in a few clicks.

## Prerequisites

- Apollia running.
- A personal or Workspace Google account.
- Your sovereignty profile is not set to `local_only` (otherwise the cloud buttons are greyed out).
- An active internet connection.

## Steps

1. In the sidebar, open **Connections**, then select the **Google Workspace** card in the list of native connectors.

   ![Connections page, Google Workspace card selected in the sidebar (Not connected state), right-hand panel with the Accounts (0) tab and the Connect an account button](/img/operator-help/integration-google-workspace-1.png)

2. Click **Connect an account**. A window opens inside Apollia and your browser automatically opens the Google consent page.

3. Pick the Google account to use, then accept the permissions offered (Mail, Calendar, Drive Workspace, and so on).

   ![Google consent screen, Apollia asks for access to the account, list of permissions (app Drive files, Calendar events, sending mail, draft management), warning that the app is not verified by Google](/img/operator-help/integration-google-workspace-2.png)

4. Back in Apollia, the window detects the return automatically. A second step offers you the agent Drive root folder (default **Apollia**). Confirm by clicking **Save** (or **Keep the default** to keep the value offered).

   ![Google Drive folder dialog in Apollia, explanation of the drive.file scope, Folder path field with the value Apollia, Keep the default and Save buttons](/img/operator-help/integration-google-workspace-3.png)

5. The window closes, your account appears in the sidebar with a green dot.

   On screen: the Connections sidebar, with the Google Workspace card expanded showing the connected account, its green dot and the Disconnect button.

## What you can do

**Reads (without approval)**: list your Gmail drafts, list your Calendar events, browse the `Apollia/` folder on Drive, read Sheets cells, read Docs text, list your tasks, search YouTube videos.

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

- **The Google consent screen shows "This app is not verified"**: that is expected in expert mode with your own OAuth app. Click **Advanced** then **Go to Apollia** to continue.
- **The Connect button is greyed out**: your sovereignty profile is `local_only`. Cloud connectors are disabled in that mode.
- **You want full Gmail or Drive read access**: those scopes are restricted (Google CASA audit) and out of scope for v0.1.0. No Apollia tool uses them yet, see "Expert mode" below.
- **The agent does not see a specific file on Drive**: it only has access to the `Apollia/<agent>/` folder. Drop the file into that folder or pass it the explicit identifier in your prompt.
- **An agent asks for a full-read Gmail tool**: there is none. The Google operation catalogue only contains non-restricted scopes, and a test locks that down. The agent will get an unknown tool error, not a scope error.

## Expert mode: your own OAuth app

This section is for power users familiar with the Google Cloud Console. If you are not one, the default perimeter already covers sending, composing, the full calendar and the scoped Drive, and you can skip it.

**Why this mode exists.** The Google scopes classified *restricted* (`gmail.readonly`, `gmail.modify`, `drive.readonly`, `drive`) require a CASA Tier 2 audit by a Google-approved third party, billed 5,000 to 15,000 dollars a year. To stay free, the default Apollia app does not request them. You can create your own OAuth app, keep it in **Testing** status (up to 100 test users), and plug it into Apollia. No cost.

**What this mode does today, and what it does not.** It plugs your OAuth client in place of the shared app, and the consent screen can then offer the restricted scopes. On the other hand **no Apollia tool uses those scopes yet**: the Google operation catalogue contains none of them, and a test locks that down. Obtaining the scope therefore unlocks no new capability for now. Use this mode if you want to own your OAuth app, not to gain features.

**Procedure.**

1. **Google Cloud Console**: create a project, enable the Gmail, Calendar and Drive APIs, configure the OAuth consent screen in External + Testing mode, add your email as a test user, add the restricted scopes you want, create a Desktop type OAuth client, note the **Client ID**.
2. **Apollia**: export the environment variable before launching Apollia:

   ```bash
   export APOLLIA_GOOGLE_CLIENT_ID="123456789-abcdef.apps.googleusercontent.com"
   ```

3. **Reconnect Google** in Apollia. The consent screen will show your app.

**Verification.** The Google consent screen shows the name of your app and not "Apollia OS", and the `granted_scopes` listed under the connected account include the granted scope.

**If it does not work.**

- **The screen still shows "Apollia OS"**: the process that launched Apollia does not have the variable. Relaunch Apollia from the shell where you ran `export`, or add the variable to your `~/.zshrc` or `~/.bashrc`.
- **Google refuses the scopes**: stay in **Testing** mode and add yourself as a test user.

**Responsibility.** In expert mode, the OAuth app is yours and Apollia does not audit that configuration. If you distribute Apollia with your Client ID embedded beyond 100 users, Google will require the CASA Tier 2 audit.

**Alternative.** If the Google Cloud Console feels heavy, a community Gmail MCP server (search for `mcp-server-gmail`) runs locally with your credentials and exposes the Gmail tools through MCP. See [Wire your own MCP server](cabler-son-propre-serveur-mcp.md).

## Disconnecting an account

On the Google Workspace card, click **Disconnect** next to the account concerned. The token is immediately deleted from the local keychain. To revoke on the Google side too, go to https://myaccount.google.com/permissions and remove the Apollia application.

> **Technical reference:** [Apollia reference](/reference) , full scopes, proactive refresh, multiple accounts, keychain storage.
