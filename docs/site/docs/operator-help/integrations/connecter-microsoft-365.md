# Connect Microsoft 365

> For any operator who wants to plug Outlook, Calendar and OneDrive into Apollia, whether the account is personal (outlook.com, hotmail.com, live.com) or professional (Microsoft 365, Entra ID).

## Prerequisites

- Apollia running.
- A Microsoft account, personal or professional.
- Your sovereignty profile is not set to `local_only`.
- If your Entra ID tenant requires administrative approval, the administrator has to pre-approve Apollia.
- An active internet connection.

## Which account type I can use

Both work natively with the Apollia app, with no extra configuration. The endpoint used (`/common/`) accepts either:

- personal Microsoft accounts (outlook.com, hotmail.com, live.com),
- professional or education accounts (Entra ID, M365 Business, M365 Developer tenant).

See the observable differences in the table further down.

## Steps

1. In the sidebar, open **Connections**, then select the **Microsoft 365** card.

   *Figure: the Connections page, with the Microsoft 365 card highlighted and the Connect an account button in the right-hand panel.*

2. Click **Connect an account**. A window opens inside Apollia and your browser opens the Microsoft consent page.

3. Authenticate with your Microsoft account, then accept the permissions (Mail, Calendar, Files).

   *Figure: the Microsoft consent page, with the list of requested accesses (Mail, Calendar, Files) and the No and Yes buttons.*

4. Back in Apollia, the window detects the return automatically and closes. Your account appears in the sidebar with a green dot.

   *Figure: the Connections sidebar, with the Microsoft 365 card expanded showing the connected account and its green dot.*

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

## Verification

- The dot next to the account is green.
- In free chat, ask *"List my last 3 Outlook mails"*. The answer comes back without an approval request.
- Then try *"Send a mail to <your address> with the subject test"*. An approval popup appears before sending.
- The system keychain holds an `apollia-connector-microsoft` entry tied to your address.

## If it does not work

- **AADSTS90094, "admin consent required"**: your Entra ID tenant requires an organization-level approval. Contact your administrator so they pre-approve Apollia, or use a personal Microsoft account.
- **AADSTS500011, "application not found in the tenant"**: your administrator has restricted external apps. Ask them to pre-approve Apollia, or use a personal account.
- **`outlook.send` fails with `ErrorRecipientNotResolved`**: Microsoft Graph validates recipients more strictly than Google. Check the target address and make sure there is no dead alias.
- **OneDrive write refused**: that is expected in v0.1.0, OneDrive is read-only.
- **The Connect button is greyed out**: your sovereignty profile is `local_only`.

## Disconnecting an account

On the Microsoft 365 card, click **Disconnect** next to the account. The token is deleted from the local keychain. To revoke on the Microsoft side too, go to https://myaccount.microsoft.com and remove the Apollia application permission.

> **Technical reference:** [Apollia reference](/reference) , Microsoft OAuth flow, full scopes, proactive refresh.
