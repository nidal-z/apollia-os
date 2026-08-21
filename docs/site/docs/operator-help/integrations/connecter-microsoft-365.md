---
title: Connect Microsoft 365
sidebar_position: 4
---

# Connect Microsoft 365

> For any operator who wants to plug Outlook, Calendar and OneDrive into Apollia, whether the account is personal (outlook.com, hotmail.com, live.com) or professional (Microsoft 365, Entra ID).

## Prerequisites

- Apollia running.
- A Microsoft account, personal or professional.
- Your sovereignty profile is not set to `local_only`.
- If your Entra ID tenant requires administrative approval, the administrator has to pre-approve the application.
- An active internet connection.

There is nothing to register and nothing to paste. If you have just read the Google page, that difference is deliberate and explained in [the integrations overview](/operator-help/integrations/vue-d-ensemble-integrations).

## Nothing to configure

<!-- claim:oauth-microsoft-client-embedded -->
Microsoft 365 works as soon as Apollia is installed. Apollia registers one application with Microsoft and ships its identifier inside the build, so you go straight to connecting your account.

That identifier is not a secret being leaked. In Microsoft's model a desktop application is a *public client*: it holds no password at all, proves each request with PKCE, and its application identifier is a public GUID. Anyone can read it out of any copy of the application, which is precisely why the design does not depend on hiding it.

You can still [use your own registration](#use-your-own-application-registration) if you prefer, and skipping straight to the steps below is the normal path.

## Which account type I can use

Both work through the same registration. The endpoint used (`/common/`) accepts either:

- personal Microsoft accounts (outlook.com, hotmail.com, live.com),
- professional or education accounts (Entra ID, M365 Business, M365 Developer tenant).

See the observable differences in the table further down.

## Steps

1. In the sidebar, open **Connections**, then select the **Microsoft 365** card.

   On screen: the Connections page, with the Microsoft 365 card highlighted and the Connect an account button in the right-hand panel.

2. Click **Connect an account**. A window opens inside Apollia and your browser opens the Microsoft consent page.

3. Authenticate with your Microsoft account, then accept the permissions (Mail, Calendar, Files).

   On screen: the Microsoft consent page, with the list of requested accesses (Mail, Calendar, Files) and the No and Yes buttons.

4. Back in Apollia, the window detects the return automatically and closes. Your account appears in the sidebar with a green dot.

   On screen: the Connections sidebar, with the Microsoft 365 card expanded showing the connected account and its green dot.

## What you can do

**Outlook (mail)**:
- Automatic reads: search messages, read a specific mail, list your folders.
- Writes with HITL approval: send a mail, reply, move a message.

**Calendar**:
- Automatic reads: list events, open a specific event.
- Writes with HITL approval: create or change an event.
- Deletion with a confirmation phrase: delete an event.

**OneDrive (read-only in v0.1.0)**:
- Search your files, read the metadata, download a file, list recent files.
- OneDrive writing and the workspace folder pattern (the equivalent of Google's `Drive/Apollia/<agent>/`) will land in a later version.

Microsoft Teams is not covered in v0.1.0.

## Multiple accounts

As with Google, you can connect several Microsoft accounts. Each account keeps its own token in the keychain. When an agent calls a Microsoft tool, it can pick the target account if several are connected.

## Personal vs professional differences

| Capability | Personal account | Professional account |
|---|---|---|
| Mail backend | Outlook.com | Exchange Online |
| Calendar backend | Outlook.com | Exchange Online |
| Drive backend | OneDrive Personal | OneDrive for Business |
| Admin consent | Not applicable | Possible depending on tenant policy |
| Domains | outlook.com, hotmail.com, live.com | `<you>@<company>.onmicrosoft.com` or a custom domain |

The tools are the same on both sides, only the backend answering changes.

## Use your own application registration

Optional. Apollia's own registration covers both account types, so most operators never need this. Two situations justify it: your organization wants the connection to appear under an application it controls and can audit, or your Entra ID tenant blocks applications it has not registered itself.

**In the Microsoft Entra admin center** (or the Azure portal, "App registrations"):

1. Choose **New registration**.
2. For supported account types, pick **Accounts in any organizational directory and personal Microsoft accounts**, which is what makes both an `outlook.com` address and a work account usable. Picking a single-tenant option instead limits sign-in to your own directory.
3. Under **Redirect URI**, add a platform of type **Mobile and desktop applications** and enter `http://127.0.0.1`. Apollia listens on a loopback port picked at connection time, and Microsoft accepts any port on that host.
4. Register, then copy the **Application (client) ID** from the overview page. It looks like `00000000-1111-2222-3333-444444444444`.

**In Apollia.**

1. Open **Settings → OAuth integrations**.
2. Paste the identifier into the client ID field on the Microsoft card and save. It is stored in `~/.apollia/oauth-clients.toml`, readable by your user only. Leave the secret field empty, a public client has none.
3. Click **Test configuration**.

Clearing the field again restores the identifier shipped with Apollia. Existing connected accounts were issued against the previous registration, so disconnect and reconnect them after a change.

**Alternative for a shell or a headless host.** `APOLLIA_MICROSOFT_CLIENT_ID` takes priority over the file, and only applies to processes launched from the shell where you exported it. See [Environment variables](/reference/environment-variables).

## Verification

- The dot next to the account is green.
- In free chat, ask *"List my last 3 Outlook mails"*. The answer comes back without an approval request.
- Then try *"Send a mail to <your address> with the subject test"*. An approval popup appears before sending.
- The system keychain holds an `apollia-connector-microsoft` entry tied to your address.

## If it does not work

- **The Microsoft 365 card reads "Setup required"**: it cannot, on a build that carries the identifier, and neither an empty environment variable nor a blanked entry in `~/.apollia/oauth-clients.toml` produces it: both are skipped when empty and the shipped identifier stands. If you do see it, the build itself was compiled without the identifier. Check `apollia-os connector list`, which names the source of the client it resolved.
- **Microsoft rejects the redirect URI**: only reachable with your own registration. It is missing its **Mobile and desktop applications** platform, or that platform does not list `http://127.0.0.1`. A registration created as "Web" will not work. Clear the client ID field to fall back on Apollia's own registration.
- **Microsoft answers that the account does not exist in the directory**: also specific to your own registration, and it means the supported account types were set to a single tenant. Recreate it with **Accounts in any organizational directory and personal Microsoft accounts**, or clear the field to use Apollia's.
- **Consent refused at the Microsoft screen**: a managed Entra ID tenant often requires an organization-level approval before an external application may be used at all. The error text comes from Microsoft, not from Apollia, and it names the tenant policy at fault. Ask your administrator to pre-approve the application, or use a personal Microsoft account.
- **`outlook.send` fails on a recipient**: Microsoft Graph validates recipients more strictly than Google does. Apollia surfaces Graph's own error verbatim, prefixed with the HTTP status. Check the target address and make sure there is no dead alias.
- **OneDrive write refused**: that is expected in v0.1.0, OneDrive is read-only.
- **The Connect button is greyed out**: your sovereignty profile is `local_only`.

## Disconnecting an account

On the Microsoft 365 card, click **Disconnect** next to the account. The token is deleted from the local keychain. To revoke on the Microsoft side too, go to https://myaccount.microsoft.com and remove the Apollia application permission.

> **Technical reference:** [Apollia reference](/reference) , Microsoft OAuth flow, full scopes, proactive refresh.
