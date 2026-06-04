# SECURITY

> Secrets, identities, audit, and the sovereignty boundary. Read this before
> writing any code that handles credentials, user data, or calls an external
> service.

Apollia is local-first by design. The boundary between local and external
is not implicit. Every crossing must be explicit, logged, and revocable.

---

## 1. The sovereignty boundary

Two operator-controlled profiles :

| Profile | Behavior |
|---|---|
| `local_only` | no outbound network calls to LLM/MCP cloud, OAuth disabled, only local backends (llama-cpp, Ollama) allowed |
| `cloud_allowed` | cloud LLMs (Anthropic, OpenAI, Vertex) and OAuth-backed connectors enabled |

State :
- The setting is **read-only in v0.1.0 UI**. Hardcoded to `cloud_allowed` in
  `crates/apollia-desktop/ui/src/routes/Connections.svelte` line 665 and
  `Integrations.svelte` line 122.
- The runtime honors the profile when set in config, regardless of UI.
- An operator profile UI switch is planned post-v0.1.0.

Rules :
- Never assume a profile. Read it from `RuntimeContext::profile()`.
- A new external call site must check the profile before issuing the
  request and surface a `ProfileViolation` error when blocked.

---

## 2. `SecretStore`

```rust
#[async_trait]
pub trait SecretStore: Send + Sync {
    async fn read(&self, key: &SecretKey) -> Result<Option<Secret>, SecretError>;
    async fn write(&self, key: &SecretKey, value: Secret) -> Result<(), SecretError>;
    async fn delete(&self, key: &SecretKey) -> Result<(), SecretError>;
    async fn list(&self) -> Result<Vec<SecretKey>, SecretError>;
}
```

Two backends :

| Backend | When | Selected via |
|---|---|---|
| `KeyringSecretStore` | default, OS keyring | implicit default |
| `AgeFileSecretStore` | headless, CI, isolated `$HOME` | `APOLLIA_TOKEN_STORAGE=file` |

Source : `crates/apollia-auth/src/secret_storage.rs`. Backend selection :
`apollia_auth::select_secret_store()`.

Rules :
- Never read or write secrets outside the `SecretStore` trait.
- Never log a `Secret`. The type does not implement `Display`, only
  `Debug` (and `Debug` redacts).
- Rotation : the `delete` + `write` pair is the atomic unit.
- `KeyringSecretStore` failures (locked keyring, container without
  D-Bus) fall back to `AgeFileSecretStore` only when explicitly
  selected. Never silently.

---

## 3. OAuth and token lifecycle

ADR-016 + ADR-018. Apollia supports multi-account OAuth2 PKCE for Google
and Microsoft. Tokens are stored via `SecretStore`. Refresh is automatic
with singleflight to prevent concurrent refresh races.

Lifecycle :

```
oauth_start_flow(provider, scopes, sovereignty)
   -> browser
oauth_complete_flow(state, code)
   -> SecretStore.write(refresh_token)
   -> token cache populated
   ... time passes, access_token expires ...
   -> auto-refresh via singleflight
   -> SecretStore.write(new_refresh_token)
```

Rules :
- The state parameter is signed and validated. Never trust unsigned state.
- `granted_scopes` returned by the IdP may differ from `requested_scopes`.
  Surface the difference to the user.
- Token storage uses the `SecretStore` trait. Never persist tokens
  elsewhere (no `~/.apollia/tokens.json`, no env vars).
- Multi-account : one `account_id` per (provider, identity). The
  `account_id` is the index into the secret store.

See `crates/apollia-auth/src/` and `docs/wiki/Briques-Auth-OAuth.md`.

---

## 4. MCP and connector credentials

MCP servers may require credentials (API keys, OAuth tokens). They are
stored via `SecretStore` under `mcp:<server_name>:<env_var>` keys.

Rules :
- An MCP server config in `~/.apollia/mcp.json` references environment
  variables by name, never values.
- The runtime resolves the env vars at spawn time from `SecretStore`.
- A failed resolution puts the MCP server in `ProcessState::DEGRADED`,
  not `STOPPED`. Operator action required.

Native SaaS connectors (Gmail, Calendar, Drive, Outlook, OneDrive) reuse
the OAuth flow above. See `crates/apollia-connectors/`.

---

## 5. Permissions and audit

The permissions engine has three layers (see
`docs/agents/ARCHITECTURE.md` §C). Every tool invocation produces a
decision record in `governance.db`.

Decision outcomes :

| Decision | Meaning |
|---|---|
| `allow` | execution proceeds, recorded |
| `ask` | execution paused, HITL prompt issued |
| `deny` | execution blocked, recorded |

Scopes : `session`, `project`, `global`. The closest scope wins.

Audit table is append-only. Never delete a row programmatically. The
operator can prune by date via `apollia audit prune --before <date>`.

ADR-015.

---

## 6. Filesystem isolation

ADR-015. Filesystem access from agents goes through `apollia-tools`
sandbox. A reversible journal logs every write.

Rules :
- Never grant filesystem access without going through the tool layer.
- Path validation : agent-supplied paths are resolved against the
  workspace root and rejected if they escape (`..`, symlinks pointing
  outside).
- The journal is the rollback path. A failed agent step that wrote files
  triggers `journal.rollback(step_id)`.

---

## 7. Network access

Outbound HTTP from agents goes through `apollia-tools` HTTP wrapper. The
wrapper applies :
- Profile gating (`local_only` blocks).
- DNS rebind protection.
- Per-host rate limits.
- Audit log entries.

Rules :
- Never `reqwest::Client::new()` directly from an agent path. Use the
  wrapper.
- Never bypass DNS validation. The wrapper resolves once and pins the
  IP for the request.
- Webhook outbound is the only direct-network path in the runtime, and
  it carries no agent payload.

---

## 8. Sensitive data in events and logs

- Never log raw secrets, OAuth tokens, API keys, PII.
- Never include sensitive values in EventBus events. EventBus is observed
  by the desktop UI and any audit subscriber.
- When you need to log identity, log a stable derived identifier (hash,
  account id prefix), not the raw value.
- See `docs/agents/OBSERVABILITY.md` §11.

---

## 9. Configuration and `.env` hygiene

- `.env`, `.apollia/`, `.keyfile`, `*credentials*`, `*token*` are in
  `.gitignore`. CI enforces.
- Apollia reads runtime config from `~/.apollia/config.toml`. Never read
  secrets from environment variables in production code (except the
  documented `APOLLIA_*` selectors).
- `APOLLIA_*` env vars : selectors only (which backend, which path),
  never raw secrets.

---

## 10. Threat model summary (operator-facing)

| Threat | Mitigation |
|---|---|
| Local file exfiltration by malicious agent | Filesystem sandbox, audit journal, reversible writes |
| Credential leak via prompt injection | `InjectionDetector` layer, `ask` decision triggered |
| Network exfiltration | Profile gating, host pinning, audit log |
| Token theft from disk | OS keyring or age-encrypted file |
| Replay of OAuth state | Signed state parameter |
| MCP server hijack | TLS validation, server cert pinning where applicable |

A more formal threat model lives in `docs/wiki/Security-Threat-Model.md`
(post-L2b).

---

## 11. When the rules block you

- Need to log a value that looks sensitive : extract a non-sensitive
  identifier and log that.
- Need to call an external service from a new code path : route through
  `apollia-tools` HTTP wrapper or open an ADR for a new wrapper category.
- Need to add a new secret kind : extend `SecretKey` in `apollia-auth`,
  update the `SecretStore` doc-comment, document the key naming in this
  file.
- Need to bypass profile gating for a legitimate reason : the answer is
  no. Open an ADR if you genuinely believe a bypass is required.
