---
title: Connect Microsoft 365
slug: /operator-help/integrations/connect-microsoft-365
sidebar_position: 4
---

# Connect Microsoft 365

> For any operator who wants to plug Outlook, Calendar and OneDrive into Apollia, whether the account is personal (outlook.com, hotmail.com, live.com) or professional (Microsoft 365, Entra ID).

## Prerequisites

- Apollia running.
- A Microsoft account, personal or professional.
- Your organisation's policy, if it has deployed one, does not deny remote connections.
- If your Entra ID tenant requires administrative approval, the administrator has to pre-approve the application.
- An active internet connection.

There is nothing to register and nothing to paste. If you have just read the Google page, that difference is deliberate and explained in [the integrations overview](/operator-help/integrations/integrations-overview).

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

1. In the sidebar, open **Connections**, click **Add a connection**, and choose **Microsoft 365** under **Offered by Apollia**. Its card says what it asks: a sign-in to your account, in your browser. Click **Add**.

2. On the **Connect** step, click **Sign in**. A window opens inside Apollia and your browser opens the Microsoft consent page. With a work account, the step warns you that your organisation may ask an administrator to approve Apollia first, and that Apollia then writes you the request to send them.

3. Authenticate with your Microsoft account, then accept the permissions (Mail, Calendar, Files).

   On screen: the Microsoft consent page, with the list of requested accesses (Mail, Calendar, Files) and the No and Yes buttons.

4. Back in Apollia, the window detects the return automatically. On **Who can use it**, check the agents that may use the account; **Apollia, in the Chat** comes checked. Click **Continue**.

5. **Try it** confirms that Microsoft 365 is added, with **Try** and **Done**. The account appears in the **Connections** list, on a line of its own, with its address.

   On screen: the card of the Microsoft account in Connections, with its state, its agents and its OneDrive choices.

## What you can do

**Outlook (mail)**:
- Reads: search messages, read a specific mail, list your folders. Reading your account is reading personal data, so Apollia asks for your agreement; **Always for this agent** stops the question for that agent.
- Writes and sends, with your agreement: send a mail, reply, move a message.

**Calendar**:
- Reads, under the same rule: list events, open a specific event.
- Writes, with your agreement: create or change an event.
- Deletion, with your agreement like any write: delete an event. There is no phrase to type.

**OneDrive (read-only)**:
- Search your files, read the metadata, download a file, list recent files.
- Only inside the folders and files you choose. On the card of the account, under **OneDrive files Apollia can read**, **Choose folders and files** lets you browse your OneDrive and pick them; until you do, your agents read nothing there, and an agent that tries is told to ask you to choose. A chosen folder opens everything under it, and **Remove** closes it again. The root of the drive itself cannot be chosen.
- The permission Microsoft grants covers your whole OneDrive. The limit is held by Apollia on your computer, which checks every item before an agent sees it.
- There is no OneDrive write, and no workspace folder pattern like Google's `Drive/Apollia/<agent>/`.

Microsoft Teams is not covered.

## Multiple accounts

As with Google, you can connect several Microsoft accounts, and each keeps its own token. On the card of an account, **Add another account** connects the next one. Each connected account is a connection of its own, on its own line of the list. Its tools are given to an agent account by account, on the card of the account with **Give to an agent** or on the agent's sheet, and a call runs as the account that was given and as no other.

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

1. Turn on developer mode. In **Connections**, open the card of a Microsoft account: its **Technical detail** section holds the **Microsoft client**. With no account connected yet, use the environment variable described below instead.
2. Paste the identifier into the **Client ID** field and save. It is kept in a configuration file of your computer, readable by your user only. There is no secret to enter, a public client has none.
3. Click **Test the client**.

Clearing the field again restores the identifier shipped with Apollia. Existing connected accounts were issued against the previous registration, so remove them and connect them again after a change.

**Alternative for a shell or a headless host.** `APOLLIA_MICROSOFT_CLIENT_ID` takes priority over what was entered on the page, and only applies to processes launched from the shell where you exported it. See [Environment variables](/reference/environment-variables).

## Verification

- The account has its line in **Connections**, and its card reads **Working** once checked or used.
- On the card, click **Try** and ask *"List my last 3 Outlook mails"*. Apollia answers with the tools of this connection, and asks for your agreement before it reads the account.
- Then try *"Send a mail to <your address> with the subject test"*. A consent request appears before sending.
- The system keychain holds an `apollia-connector-microsoft` entry tied to your address.

## If it does not work

- **The Microsoft 365 card reads "Setup required before connecting"**: it cannot, on a build that carries the identifier, and neither an empty environment variable nor an emptied **Client ID** field produces it: both are skipped when empty and the shipped identifier stands. If you do see it, the build itself was compiled without the identifier. Check `apollia-os connector client-id list`, which names the source of the client it resolved, `env`, `file`, `builtin` or `none`. Plain `connector list` shows the registered connectors, their services and their operation counts, and says nothing about the client identifier.
- **Microsoft rejects the redirect URI**: only reachable with your own registration. It is missing its **Mobile and desktop applications** platform, or that platform does not list `http://127.0.0.1`. A registration created as "Web" will not work. Clear the client ID field to fall back on Apollia's own registration.
- **Microsoft answers that the account does not exist in the directory**: also specific to your own registration, and it means the supported account types were set to a single tenant. Recreate it with **Accounts in any organizational directory and personal Microsoft accounts**, or clear the field to use Apollia's.
- **Blocked by your organisation**: depending on its consent settings, an organisation may require an administrator to approve Apollia before a work account may read its mail and calendar. When Microsoft says so, the window reads **Blocked by your organisation**, with the approval address (**Copy the address**) and a request written for your administrator, ready to send (**Copy the request**). Microsoft 365 then stays in the **Connections** list as **Blocked by your organisation**; the address and the request stay on its card there, so you can close the window and copy them later. Send the address or the request to your administrator: they open the address, sign in, and approve Apollia for the whole organisation. Then click **Retry** on that card, or connect again from the **Microsoft 365** entry of **Add a connection**; the block lifts as soon as an account connects. If you refused the consent with a work account, Apollia also offers that address, without marking the connection blocked: Microsoft does not tell a refusal apart from an approval left unasked. A personal account (outlook.com, hotmail.com, live.com) approves for itself and never sees this. Apollia asks for your own files only (`Files.Read`), never for every file shared with you, which consent settings more often keep for an administrator.
- **`outlook.send` fails on a recipient**: Microsoft Graph validates recipients more strictly than Google does. Apollia surfaces Graph's own error verbatim, prefixed with the HTTP status. Check the target address and make sure there is no dead alias.
- **OneDrive write refused**: that is expected, OneDrive is read-only.
- **Connecting stops with "Refused by your organisation's policy"**: your organisation's policy denies connections to online services on this computer, and nothing was sent to Microsoft. Only the person who manages that policy can grant an exception.

## Removing an account

On the card of the account, click **Remove**. The card lists what goes with it, such as its sign-in and the tools each agent held through it, and its OneDrive choices go too. Confirm with **Remove the connection**. The token is deleted from the local secret store. To revoke on the Microsoft side too, go to https://myaccount.microsoft.com and remove the Apollia application permission. See [Pause or remove a connection](pause-or-remove-a-connection.md).

> **Technical reference:** [Apollia reference](/reference) , Microsoft OAuth flow, full scopes, proactive refresh.
