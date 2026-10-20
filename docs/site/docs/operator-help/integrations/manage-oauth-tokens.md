---
title: Manage OAuth tokens
slug: /operator-help/integrations/manage-oauth-tokens
sidebar_position: 10
---

# Manage OAuth tokens

> For any operator who wants to know where Apollia stores its OAuth tokens, how to inspect them, revoke them, and understand automatic refresh.

## Prerequisites

- At least one connected account (Google, Microsoft or an OAuth MCP server).
- Access to your system's keyring tool (Keychain Access, Credential Manager, `secret-tool`).

## Where my tokens are stored

By default, Apollia stores every OAuth token in your system keyring.

| System | Backend | How to inspect |
|---|---|---|
| macOS | Keychain Services | **Keychain Access** application, search `apollia-connector-` |
| Windows | Credential Manager | **Windows Credential Manager**, **Generic Credentials** |
| Linux | Secret Service (gnome-keyring or KWallet via D-Bus) | `secret-tool search service apollia-connector-google` |

Naming convention:

- Service: `apollia-connector-<provider>` (for example `apollia-connector-google`, `apollia-connector-microsoft`).
- User: the account identifier, typically the email address.
<!-- claim:mcp-oauth-uses-one-keyring-service -->
- For OAuth MCP servers, a single service `apollia-mcp-oauth`, with the server name in the account slot.

An index at `~/.apollia/connectors-index.json` lists the connected accounts per provider (most keyrings do not support native enumeration).

## Inspect a token

1. Open your system's keyring tool.
2. Search for `apollia-connector-`.
3. Double-click the entry of the account concerned.
4. The content is a serialized JSON with `access_token`, `refresh_token`, `expires_at`, `scopes`.

## Revoke an account

**On the Apollia side (local keyring)**:

1. Open **Connections**.
2. Open the page of the connection, Google Workspace or Microsoft 365.
3. Under the **Accounts** tab, click **Disconnect** next to the account and confirm. The token is removed immediately from the secret store and from the index.

The token stays valid on the Google or Microsoft side until its natural expiry (typically one hour for the access token).

**On the provider side (full revocation)**:

- Google: https://myaccount.google.com/permissions, click Apollia, **Remove access**.
- Microsoft: https://myaccount.microsoft.com/consent, find Apollia, **Remove permission**.

This operation also invalidates the refresh token. Recommended for a clean revocation.

## Multi-account

Each account lives in its own keyring entry with the email as user. Each connected account is a connection of its own: its tools are granted to an agent account by account, on the agent's sheet, and a call runs as the account that was granted and as no other.

## Automatic refresh

Apollia refreshes tokens proactively:

- The refresh is triggered 60 seconds before the access token expires.
- A **singleflight** protection: if several concurrent calls trigger a refresh on the same account, a single HTTP request is sent to the provider. Without this protection, a burst of agent calls would fire N parallel requests and trigger a rate-limit cascade.

## Change the scopes of an account

You cannot. The scope set of a native connector is fixed in the application: the Google dialog always asks for the same ten scope aliases and the Microsoft one for the same six, and there is no checkbox to adjust before connecting. Disconnecting and reconnecting replays exactly the same request. Narrowing what Apollia may do is done on the provider side, by restricting the OAuth client, or by not connecting the account at all.

## Verification

- On the page of the connection, the account no longer appears after disconnection.
- The system keyring tool no longer shows a matching entry.
- A native tool call on the revoked account returns `NotConnected`.

<details>
<summary>Advanced configuration</summary>

### Headless Linux: connector tokens without a keyring

On a Linux box without a graphical environment (Docker container, minimal VM, server distribution), the Secret Service keyring is usually not available. Connector tokens go through the same secret store as every other secret of Apollia, and the `APOLLIA_TOKEN_STORAGE` environment variable selects that store.

Set `APOLLIA_TOKEN_STORAGE=file` and `APOLLIA_TOKEN_PASSPHRASE` before starting Apollia: the Google and Microsoft tokens are then written as encrypted files under `~/.apollia/secrets/`, one per account, instead of the keyring. The passphrase is mandatory in this mode. Without it, Apollia refuses the store rather than falling back to something weaker.

The other option on a headless machine is to run a Secret Service implementation (`gnome-keyring-daemon --components=secrets`, unlocked at session start) and to keep the default store. In file mode the keyring tools named above show nothing: the tokens are the files of that folder.

### Action audit

<!-- claim:journal-is-the-execution-record -->
Every tool call of a run is recorded in the signed journal, `~/.apollia/audit_journal.db`: the agent, the tool, its family of action, its target, its duration and whether it succeeded. Read it from **Activity log** in the sidebar.

### Common errors in file mode

- **The account cannot be saved on Linux**: no Secret Service daemon is available and the file mode is not set, or it is set without its passphrase. See the headless Linux section above.
- **A refresh reports no refresh token available**: the provider returned none. It is not a scope you forgot: Apollia always adds `access_type=offline` for Google and always includes `offline_access` for Microsoft, so that cause is not reachable from the interface. The usual explanation is a grant already revoked on the provider side. Disconnect then reconnect.
- **Refresh looping on 401**: the refresh token was revoked on the provider side. Disconnect then reconnect.

</details>

## If it does not work

- **Linux, the account cannot be saved**: see the headless Linux section.
- **A refresh reports no refresh token available**: reconnect the account. The scopes are not the cause, Apollia always asks for offline access.
- **Refresh looping on 401**: disconnect then reconnect the account, the refresh token was revoked on the provider side.

> **Technical reference:** [Apollia reference](/reference) , keyring storage, proactive refresh, governance.db audit.
