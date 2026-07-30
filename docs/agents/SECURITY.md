# SECURITY

> Secrets, identities, audit, and the sovereignty boundary. Read this before
> writing any code that handles credentials, user data, or calls an external
> service.

Apollia is local-first by design. The boundary between local and external
is not implicit. Every crossing must be explicit, logged, and revocable.

---

## 0. The agent trust model (read this first)

Agent Python code runs in-process through the PyO3 bridge, with the full rights
of the runtime process, which are the rights of the current user. There is no
OS sandbox around agent code: no process-per-agent isolation, no seccomp, no
namespaces confining the agent itself. A deliberately malicious or buggy agent
can read the filesystem, open sockets, spawn processes, and read credentials
from the keyring, regardless of what it declares in its manifest. This is the
v0.1.0 trust model recorded in ADR-003, and it is deliberate: the audience is
advanced builders who write or audit their own agents.

What this means in practice:

- Security is procedural before it is technical. The operator audits an agent
  before installing it. The install path prints a trust banner to that effect.
- The real gate for sensitive actions is HITL: the permission engine defaults
  to `NeedsApproval`, so a write or an external call surfaces an approval rather
  than executing silently. See section 5.
- Manifest capability allowlists (`tools_required`, `secrets`, `datasources`,
  `mailbox`) gate the `ctx.*` convenience interfaces. They are least-privilege
  ergonomics, not an OS boundary: an unsandboxed agent can bypass `ctx.secrets`
  with raw `os.environ` or `open()`. Never describe them as isolation.
- Native tools (`bash_executor`, `python_executor`) are the confined surface,
  not the agent. On Linux they run under PID and mount namespaces via `unshare`;
  on macOS there is no OS sandbox and a per-invocation warning is emitted. On
  every Unix platform their child processes carry per-process resource limits
  (CPU, address space, file descriptors) via `setrlimit`. See sections 6 and 7.

Never imply, in code, comments, or public docs, that agent code is sandboxed.
The honest posture is a feature for a regulated adopter; overstating it is a
liability.

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
- **Enforcement is not yet wired in v0.1.0.** The sovereignty value is collected
  at onboarding and shapes the permission rules proposed to the operator, but no
  runtime gate blocks a cloud call on `local_only`. The connector error
  `SovereigntyBlocked` exists as a last-resort variant but is not yet
  constructed by any call site. Treat data residency as operator-configured
  permission rules plus HITL, not an automatic profile switch.
- An operator profile UI switch and a real enforcement gate are planned
  post-v0.1.0.

Rules :
- A new external call site should route through the permission engine (HITL) so
  the operator can deny it. Do not claim the sovereignty profile blocks the call
  automatically until the enforcement gate lands.

---

## 2. `SecretStore`

```rust
pub trait SecretStore: Send + Sync {
    fn set(&self, service: &str, user: &str, value: &str) -> Result<(), AuthError>;
    fn get(&self, service: &str, user: &str) -> Result<Option<String>, AuthError>;
    fn delete(&self, service: &str, user: &str) -> Result<(), AuthError>;
    fn backend_id(&self) -> &'static str;
}
```

The trait is synchronous (the underlying keyring crate is sync). The key is
the `(service, user)` pair. Errors are `AuthError`.

Two backends :

| Backend | When | Selected via |
|---|---|---|
| `KeyringSecretStore` | default, OS keyring | implicit default |
| `AgeFileSecretStore` | headless, CI, isolated `$HOME` | `APOLLIA_TOKEN_STORAGE=file` |

Source : `crates/apollia-auth/src/secret_storage.rs`.

Rules :
- Never read or write secrets outside the `SecretStore` trait.
- Never log a secret value. Values are plain strings on this boundary;
  keep them out of tracing fields and error messages.
- Rotation : the `delete` + `set` pair is the atomic unit.
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

See `crates/apollia-auth/src/` (OAuth2 PKCE, keyring token storage) and ADR-016.

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

What actually gates a tool call in the shipped runtime :

1. **Prefix rules** (persisted in `governance.db`). Auto-approve or auto-deny
   by argument prefix (`arg.starts_with(prefix)`); a rule with no prefix
   matches any argument. This is where the desktop "always allow" button writes
   its rules.
2. **Code-executor guard** (`apollia_permissions::executor_guard`). The
   invariant described in the next section, applied on every dispatch.
3. **HITL approval**. Anything not auto-approved reaches the user.

`apollia-permissions` also contains a `PermissionEngine` that aggregates a
`SafeList` and an `InjectionDetector` in front of the prefix rules. **It is not
wired.** `ToolDispatcher::with_permission_engine` has no caller in this
workspace, so those two layers never execute and
`PermissionDecision::AutoDeniedInjection` is unreachable in the shipped binary.
The `[permissions]` keys `safe_commands` and `injection_detection` are therefore
inert unless an embedder installs the engine itself.

Two consequences worth stating plainly, because they are easy to get wrong :

- The anti-chaining protection usually credited to the injection detector is in
  fact delivered by `executor_guard::is_single_simple_command`, which is live.
- `InjectionDetector` detects **shell** injection (CWE-77/78), never prompt
  injection. There is no prompt-injection defence in the codebase; see the
  threat table below.

### Code executors are never blanket-authorized

`bash_executor` and `python_executor` take a single argument that is an
unparsed, arbitrary-code payload (a shell line, Python source). For those tools
a grant scoped only by name is a blank check over an entire interpreter, and a
raw-string prefix is escapable by chaining (`git` would match
`git status; rm -rf ...`). The permission model enforces one invariant for them
(the set is `apollia_permissions::CODE_EXECUTOR_TOOLS`) :

- A no-prefix rule never auto-approves a code executor: "always allow" is
  downgraded to a per-invocation approval, in the chat path and the agent path
  alike. The current call is still approved once; the next one asks again.
- A prefix rule on a code executor matches only when the argument is a single
  simple command, with no chaining, pipe, redirection, substitution, or
  backgrounding (`;`, `|`, `&`, `` ` ``, `>`, `<`, `(`, `)`, `{`, `}`, `$(`,
  `${`, newline). So an approved prefix stays bound to the command the operator
  reviewed.

### Risk classifier is opt-in

`bash_executor` also runs a `RiskClassifier` before spawning, but its pattern
lists are **empty by default** (operator-configured in `apollia.toml`
`[tools.bash]`). It is a case-sensitive substring matcher, easy to bypass
(double spaces, quoting, variable expansion), so it is a defense-in-depth net,
not the primary control. The primary control against an over-broad grant is the
permission scoping above, not the classifier. Default patterns are deliberately
left empty to preserve the opt-in, local-first posture.

Audit table is append-only. Never delete a row programmatically.
Retention is time-based via `[audit].retention_days` in the config, not a
manual purge command.

ADR-015.

---

## 6. Filesystem isolation

ADR-015. The file tools (`file_read`, `file_write`, `file_edit`, `file_list`,
`file_glob`, `file_grep`, notebook read/edit) resolve every path against a
`SandboxRoot` jail and reject escapes. A reversible journal logs every write.

This jail applies to those tools only. `bash_executor` and `python_executor`
spawn child processes that dereference paths themselves, so a string-level jail
cannot constrain them; they rely on the Linux namespaces, rlimits, and HITL
described in section 0 instead. And an in-process agent can touch the filesystem
directly with raw Python, bypassing the tool layer entirely.

Rules :
- Prefer the file tools for agent filesystem access so the jail and journal
  apply.
- Path validation : file-tool paths are resolved against the sandbox root and
  rejected if they escape (`..`, symlinks pointing outside).
- The journal is the rollback path. A failed agent step that wrote files
  through the file tools triggers `journal.rollback(step_id)`.

---

## 7. Network access

Outbound HTTP from the `http_fetch` and `web_read` tools goes through the
`apollia-tools` SSRF guard (`apollia_tools::ssrf`). The guard applies :
- A name-level check (`assert_public`) that rejects loopback, RFC 1918 private,
  link-local (cloud metadata), multicast, unique-local, and internal-domain
  destinations, on the initial URL.
- The same check on the target of every redirect hop
  (`public_redirect_policy`), with a bounded hop count, so a public endpoint
  cannot `302` the client onto a private host.
- Audit log entries.

Residual (not yet mitigated) : the check is name-level only, so DNS rebinding
(host resolves public at check-time, private at connect-time) and a redirect
host that rebinds between the policy check and the socket connect are not
closed. That requires a custom resolver that pins the resolved IP for the
connection. Profile gating (`local_only`) is not yet enforced here (see
section 1). And because agent code is unsandboxed, an agent can open a raw
socket directly, bypassing the guard; it protects tool-mediated fetches, not
the agent process. Tool child processes on Linux share the host network
namespace (no `--net` isolation yet).

Rules :
- Never `reqwest::Client::new()` without the SSRF redirect policy from a tool
  path. Build the client with `apollia_tools::ssrf::public_redirect_policy`.
- Never validate only the initial URL. Redirect targets are attacker-controlled
  and must be re-checked per hop.
- Webhook outbound is the only direct-network path in the runtime, and
  it carries no agent payload. It applies the same initial + per-redirect SSRF
  check.

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
| Local file exfiltration by a deliberately malicious agent | Not technically prevented: agent code runs in-process (section 0). Defense is the install-chain audit plus HITL on tool actions. The filesystem sandbox and audit journal apply to native tool calls, not to raw agent Python |
| Buggy agent writing outside its intent via file tools | File-tool path jail (`SandboxRoot`), audit journal, reversible writes |
| Credential leak via prompt injection | **Not mitigated.** No prompt-injection defence exists: fetched content feeds the model context as data with no output-side scanning (`web_read` documents this in-module). The only barrier is that any resulting tool call still goes through the prefix rules, the code-executor guard and HITL, so injected instructions cannot silently execute |
| Network exfiltration | Profile gating, host pinning, audit log |
| Token theft from disk | OS keyring or age-encrypted file |
| Replay of OAuth state | Signed state parameter |
| MCP server hijack | TLS validation, server cert pinning where applicable |
| Crash / DoS via a crafted parser input (LLM output, web content, automation text, tool specs) | `cargo-fuzz` targets on the untrusted-input parsers, char-boundary-safe slicing, panic-free parse contract (see `docs/agents/TESTING.md` 8b) |

The agent trust model and sandbox posture are covered in
`docs/site/docs/explanation/agent-trust-model.md` and ADR-003.

---

## 11. When the rules block you

- Need to log a value that looks sensitive : extract a non-sensitive
  identifier and log that.
- Need to call an external service from a new code path : route through
  `apollia-tools` HTTP wrapper or open an ADR for a new wrapper category.
- Need to add a new secret kind : define its `(service, user)` naming
  convention, update the `SecretStore` doc-comment, and document the key
  naming in this file.
- Need to bypass profile gating for a legitimate reason : the answer is
  no. Open an ADR if you genuinely believe a bypass is required.
