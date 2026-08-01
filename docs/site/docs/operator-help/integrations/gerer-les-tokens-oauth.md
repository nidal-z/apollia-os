# Manage OAuth tokens

> For any operator who wants to know where Apollia stores its OAuth tokens, how to inspect them, revoke them, and understand automatic refresh.

## Prerequisites

- At least one connected account (Google, Microsoft or an OAuth MCP server).
- Access to your system's keyring tool (Keychain Access, Credential Manager, `secret-tool`).

## Where my tokens are stored

Apollia stores every OAuth token in your system keyring.

| System | Backend | How to inspect |
|---|---|---|
| macOS | Keychain Services | **Keychain Access** application, search `apollia-connector-` |
| Windows | Credential Manager | **Windows Credential Manager**, **Generic Credentials** |
| Linux | Secret Service (gnome-keyring or KWallet via D-Bus) | `secret-tool search service apollia-connector-google` |

Naming convention:

- Service: `apollia-connector-<provider>` (for example `apollia-connector-google`, `apollia-connector-microsoft`).
- User: the account identifier, typically the email address.
- For OAuth MCP servers, service `apollia.mcp.<server-id>`.

An index at `~/.apollia/connectors-index.json` lists the connected accounts per provider (most keyrings do not support native enumeration).

## Inspect a token

1. Open your system's keyring tool.
2. Search for `apollia-connector-`.
3. Double-click the entry of the account concerned.
4. The content is a serialized JSON with `access_token`, `refresh_token`, `expires_at`, `scopes`.

## Revoke an account

**On the Apollia side (local keyring)**:

1. Open **Connections**.
2. Select the account.
3. Click **Disconnect**. The token is removed immediately from the keyring and from the index.

The token stays valid on the Google or Microsoft side until its natural expiry (typically one hour for the access token).

**On the provider side (full revocation)**:

- Google: https://myaccount.google.com/permissions, click Apollia, **Remove access**.
- Microsoft: https://myaccount.microsoft.com/consent, find Apollia, **Remove permission**.

This operation also invalidates the refresh token. Recommended for a clean revocation.

## Multi-account

Each account lives in its own keyring entry with the email as user. When an agent calls a native tool (`gmail.send`, `outlook.search`, etc.), it can pass an `account` parameter to pick the account. Without that parameter, the primary account (the first one connected) is used.

## Automatic refresh

Apollia refreshes tokens proactively:

- The refresh is triggered 5 minutes before the access token expires.
- A **singleflight** protection: if several concurrent calls trigger a refresh on the same account, a single HTTP request is sent to the provider. Without this protection, a burst of agent calls would fire N parallel requests and trigger a rate-limit cascade.

## Change the scopes of an account

v0.1.0 does not support automatic step-up auth (requesting only the new scopes). Procedure:

1. Disconnect the account in **Connections**.
2. Reconnect, adjusting the checkboxes before clicking **Connect**.

## Verification

- On the connector card, the account no longer appears after disconnection.
- The system keyring tool no longer shows a matching entry.
- A native tool call on the revoked account returns `NotConnected`.

<details>
<summary>Advanced configuration</summary>

### Headless Linux: connector tokens need a keyring

On a Linux box without a graphical environment (Docker container, minimal VM, server distribution), the Secret Service keyring is not available, and **connector accounts cannot be stored in `v0.1.0-preview`**. Connecting Google or Microsoft on such a machine fails at the point where the token is saved.

An encrypted-file backend exists in the codebase, selected by `APOLLIA_TOKEN_STORAGE=file`, and it does not apply here: the connector token path calls the OS keyring directly instead of going through the selectable store, so setting the variable changes nothing for Google or Microsoft accounts. Do not rely on it as a workaround.

The options on a headless machine are to run a Secret Service implementation (`gnome-keyring-daemon --components=secrets`, unlocked at session start), or to connect the accounts on a desktop machine instead.

### Action audit

Every tool execution is logged in `governance.db` (table `tool_executions`) with timestamp, agent, tool, account hash, approval status, latency. Viewable through **Settings, Action history**.

MCP approvals (durable HITL acceptances) are stored separately in `~/.apollia/mcp-approvals.db`.

### Common errors in file mode

- **`keyring: no entry`**: the Secret Service daemon is not available. See the headless Linux section above: there is no workaround in `v0.1.0-preview`.
- **`NoRefreshToken`** on refresh: the account was connected without `offline_access` (Microsoft) or without `access_type=offline` (Google). Reconnect it.
- **Refresh looping on 401**: the refresh token was revoked on the provider side. Disconnect then reconnect.

</details>

## If it does not work

- **Linux, "keyring: no entry"**: see the headless Linux section.
- **`NoRefreshToken`**: reconnect the account, the `offline_access` scope was forgotten.
- **Refresh looping on 401**: disconnect then reconnect the account, the refresh token was revoked on the provider side.

> **Technical reference:** [Apollia reference](/reference) , keyring storage, proactive refresh, governance.db audit.
