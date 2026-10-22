/**
 * Typed Tauri command wrappers for the Connections surface.
 *
 * Every `invoke()` used by the Connections route and its sub-components goes
 * through this module. Wrapper exports are camelCase; the underlying Rust
 * command names are snake_case. Error handling stays at the call site.
 */
import { invoke } from "@tauri-apps/api/core";
import type {
  AgentListItem,
  Connection,
  ConnectorEnrichmentView,
  McpConnectionTestResponse,
  McpOAuthAccount,
  McpOAuthDiscoveryResult,
  McpServerConfigInput,
  McpServerDetailView,
  McpServerStatusView,
  RegistryServerView,
} from "$lib/types";

/** Native OAuth provider identifier. */
export type ProviderId = "google" | "microsoft";

export interface OauthStartResult {
  auth_url: string;
  state: string;
  callback_port: number;
}

export interface OauthCompleteResult {
  provider: string;
  account_id: string;
  granted_scopes: string[];
}

export interface McpEnrichmentEntry {
  package_identifier: string;
  enrichment: ConnectorEnrichmentView;
}

// ── Connections ─────────────────────────────────────────────────────────────

/** Every connection of this machine, as `GET /api/v1/connections` answers it. */
export function listConnections(): Promise<Connection[]> {
  return invoke<Connection[]>("list_connections");
}

/** One agent holding tools of a connection, as the grant route answers it. */
export interface ConnectionAgent {
  id: string;
  name: string;
  enabled: boolean;
  /** Whether it holds these tools through the default access. */
  byDefault: boolean;
  tools: string[];
}

/** The agents holding the tools of one connection. */
export interface ConnectionAgents {
  connection: string;
  agents: ConnectionAgent[];
  sharedWith: string[];
}

/**
 * Give `tools` of the connection `connectionId` to every agent of `agents`.
 * The runtime gives each agent what it does not hold yet and journals it; an
 * agent on the default access keeps it, the tools held on top of it.
 */
export function grantConnection(
  connectionId: string,
  agents: string[],
  tools: string[],
): Promise<ConnectionAgents> {
  return invoke<ConnectionAgents>("grant_connection", { connectionId, agents, tools });
}

/** Add the folder at `path`, chosen with the system picker, as a connection. */
export function addFolderConnection(path: string): Promise<Connection> {
  return invoke<Connection>("add_folder_connection", { path });
}

/** What a removal erases: its configuration, a token, a secret, installed files, a tool granted to an agent, a source, its state. */
export type RemovedKind =
  | "configuration"
  | "installed_files"
  | "token"
  | "secret"
  | "agent_access"
  | "source"
  | "state";

/** One thing a removal erased, or would erase. */
export interface RemovedItem {
  kind: RemovedKind;
  /** The server, the folder of installed files, the secret key, the tool, the source. */
  name: string;
  /** The agent the tool is taken from, for an agent access. */
  agent?: string | null;
}

/** What a removal erased, or on a dry run would erase. */
export interface ConnectionRemoval {
  connection: string;
  dryRun: boolean;
  removed: RemovedItem[];
  /** What could not be erased; empty on a dry run. */
  left: RemovedItem[];
}

/** Check the connection `connectionId` now; answers it with the state found. */
export function checkConnection(connectionId: string): Promise<Connection> {
  return invoke<Connection>("check_connection", { connectionId });
}

/** Put the connection `connectionId` in pause; its agents keep their accesses. */
export function pauseConnection(connectionId: string): Promise<Connection> {
  return invoke<Connection>("pause_connection", { connectionId });
}

/** Resume the connection `connectionId`. */
export function resumeConnection(connectionId: string): Promise<Connection> {
  return invoke<Connection>("resume_connection", { connectionId });
}

/**
 * Remove the connection `connectionId`, whatever its kind, with what it
 * leaves behind. With `dryRun` nothing is erased and the answer lists what
 * would be, for the confirmation.
 */
export function removeConnection(connectionId: string, dryRun: boolean): Promise<ConnectionRemoval> {
  return invoke<ConnectionRemoval>("remove_connection", { connectionId, dryRun });
}

// ── MCP servers ────────────────────────────────────────────────────────────

export function listMcpServers(): Promise<McpServerStatusView[]> {
  return invoke<McpServerStatusView[]>("list_mcp_servers");
}

export function listMcpEnrichments(): Promise<McpEnrichmentEntry[]> {
  return invoke<McpEnrichmentEntry[]>("list_mcp_enrichments");
}

export function listAgents(): Promise<AgentListItem[]> {
  return invoke<AgentListItem[]>("list_agents");
}

export function getMcpServerDetail(name: string): Promise<McpServerDetailView> {
  return invoke<McpServerDetailView>("get_mcp_server_detail", { name });
}

export function restartMcpServer(name: string): Promise<McpServerStatusView> {
  return invoke<McpServerStatusView>("restart_mcp_server", { name });
}

export function removeMcpServer(name: string): Promise<void> {
  return invoke<void>("remove_mcp_server", { name });
}

export function addMcpServer(config: Record<string, unknown>): Promise<void> {
  return invoke<void>("add_mcp_server", { config });
}

export function testMcpConnection(
  config: McpServerConfigInput | Record<string, unknown>,
): Promise<McpConnectionTestResponse> {
  return invoke<McpConnectionTestResponse>("test_mcp_connection", { config });
}

// ── Catalogue ───────────────────────────────────────────────────────────────

export function fetchMcpCurated(): Promise<RegistryServerView[]> {
  return invoke<RegistryServerView[]>("fetch_mcp_curated");
}

export function fetchMcpRegistry(): Promise<RegistryServerView[]> {
  return invoke<RegistryServerView[]>("fetch_mcp_registry");
}

// ── Native OAuth connectors ─────────────────────────────────────────────────

/**
 * Start a cloud OAuth flow.
 *
 * Whether the organisation allows the connection is decided by the runtime,
 * from the policy it loaded: a refusal comes back as `refused_by_policy`.
 */
export function oauthStartFlow(
  provider: ProviderId,
  scopes: string[],
): Promise<OauthStartResult> {
  return invoke<OauthStartResult>("oauth_start_flow", { provider, scopes });
}

export function oauthCompleteFlow(
  state: string,
  code: string,
): Promise<OauthCompleteResult> {
  return invoke<OauthCompleteResult>("oauth_complete_flow", { state, code });
}

export function oauthSetDriveFolder(
  accountId: string,
  folderPath: string,
): Promise<void> {
  return invoke<void>("oauth_set_drive_folder", { accountId, folderPath });
}

// ── MCP server config editing ───────────────────────────────────────────────

/** Persisted MCP server config (matches the backend `McpServerConfig`). */
export interface McpServerRawConfig {
  name: string;
  command: string;
  args: string[];
  env: Record<string, string>;
  transport: string;
  url?: string | null;
  init_timeout_secs: number;
  call_timeout_secs: number;
  tags: string[];
}

/** Stored config of one MCP server, secrets redacted. */
export function getMcpServerRawConfig(name: string): Promise<McpServerRawConfig> {
  return invoke<McpServerRawConfig>("get_mcp_server_raw_config", { name });
}

/** Replace the stored config of one MCP server. */
export function updateMcpServerConfig(
  name: string,
  config: McpServerRawConfig,
): Promise<void> {
  return invoke<void>("update_mcp_server_config", { name, config });
}

/** Store a secret env value for a server without echoing it back. */
export function storeMcpSecret(
  serverName: string,
  envVar: string,
  value: string,
): Promise<void> {
  return invoke<void>("store_mcp_secret", { serverName, envVar, value });
}

// ── MCP OAuth sign-in ───────────────────────────────────────────────────────

/** Probe a remote server's authorization server metadata. */
export function mcpOauthDiscover(
  url: string,
  wwwAuthenticate: string | null,
): Promise<McpOAuthDiscoveryResult> {
  return invoke<McpOAuthDiscoveryResult>("mcp_oauth_discover", {
    url,
    wwwAuthenticate,
  });
}

/** Resolve a pre-registered client id from the host environment. */
export function mcpOauthResolveClientId(envVar: string): Promise<string | null> {
  return invoke<string | null>("mcp_oauth_resolve_client_id", { envVar });
}

/** Parameters accepted by {@link mcpOauthLogin}. */
export interface McpOauthLoginArgs {
  serverName: string;
  serverUrl: string;
  wwwAuthenticate: string | null;
  scopes: string[];
  clientId?: string | null;
}

/** Run the interactive OAuth flow for a remote MCP server. */
export function mcpOauthLogin(args: McpOauthLoginArgs): Promise<McpOAuthAccount> {
  return invoke<McpOAuthAccount>("mcp_oauth_login", { ...args });
}

// ── Capability coaching (connector wizard) ──────────────────────────────────

/** One usage example the coaching step displays for a fresh connector. */
export interface CoachingExample {
  title: string;
  description: string;
  prompt: string;
}

/**
 * Asks the runtime for the post-install usage examples of an MCP server.
 *
 * `meta_generate_capabilities_coaching` takes a single structured argument
 * named `request`, so the payload has to be nested under that key: Tauri
 * looks up argument names one by one and rejects the whole call when one is
 * absent. `CoachingRequest` carries `#[serde(rename_all = "camelCase")]`,
 * hence the camelCase fields inside the nesting. The argument shape is frozen
 * by `WizardStepCoaching.test.ts`.
 */
export async function metaGenerateCapabilitiesCoaching(
  serverName: string,
  serverTitle: string | null,
): Promise<CoachingExample[]> {
  return await invoke<CoachingExample[]>("meta_generate_capabilities_coaching", {
    request: {
      serverName,
      serverTitle: serverTitle ?? serverName,
    },
  });
}

// ── Connector installation (wizard finalize step) ───────────────────────────

/** A secret typed in the wizard, waiting to be filed in the OS keyring. */
export interface PendingSecret {
  envVar: string;
  value: string;
}

/** The subset of `@tauri-apps/api/core::invoke` the install sequence needs. */
export type WizardInvoke = <T>(
  cmd: string,
  args?: Record<string, unknown>,
) => Promise<T>;

/**
 * File the connector's secrets, then install its configuration, the warning
 * shown before the addition accepted (the desktop journals it).
 *
 * Both calls carry `config.name`, and that is the whole point of this
 * function existing. The keyring key is written as `{server_name}:{env_var}`
 * by `SecretStore::key_for` and rebuilt, at resolution time, from the name
 * carried by the *installed configuration*
 * (`apollia-mcp::config::resolve_env` passes `&self.name` to
 * `resolve_single_var`). Filing a secret under any other identifier, the
 * registry one included, hides it from the only lookup that will ever go
 * looking for it, and the connector answers `UnresolvedEnvVar` on first use.
 *
 * `invokeFn` is injectable so the wire calls can be frozen by a test; the
 * default is the real Tauri bridge.
 */
export async function installConnector(
  config: McpServerConfigInput,
  secrets: readonly PendingSecret[],
  invokeFn: WizardInvoke = invoke,
): Promise<void> {
  for (const secret of secrets) {
    await invokeFn("store_mcp_secret", {
      serverName: config.name,
      envVar: secret.envVar,
      value: secret.value,
    });
  }
  await invokeFn("add_mcp_server", { config, warningAccepted: true });
}

/** Where Node.js stands in the engine store, as `node_runtime_state` answers. */
export type NodeRuntimeState =
  | { state: "absent" }
  | { state: "downloading"; received: number; total: number }
  | { state: "ready"; tag: string; path: string }
  | { state: "failed"; error: string };

/** Read where Node.js stands, to show its download while an addition waits on it. */
export function nodeRuntimeState(): Promise<NodeRuntimeState> {
  return invoke<NodeRuntimeState>("node_runtime_state");
}

// ── Mailbox connection (IMAP) ───────────────────────────────────────────────

/** A provider whose IMAP server and port are known from the address. */
export interface ImapProvider {
  name: string;
  /** Address domains it serves, lowercase. */
  domains: string[];
  host: string;
  port: number;
}

/** The providers known from the address, and the port of any other server. */
export interface ImapProviders {
  providers: ImapProvider[];
  defaultPort: number;
}

/** What the add form sends; the password is never kept by the window. */
export interface MailboxInput {
  email: string;
  host: string | null;
  port: number | null;
  password: string;
}

/** A mailbox the runtime verified and stored. */
export interface MailboxAccount {
  connectionId: string;
  email: string;
  host: string;
  port: number;
  username: string;
}

/** The providers whose server the add form fills from the address. */
export function listImapProviders(): Promise<ImapProviders> {
  return invoke<ImapProviders>("list_imap_providers");
}

/**
 * Add a mailbox: the runtime logs in for real, then stores the connection and
 * keeps the password in the secret store. A failed login stores nothing and
 * rejects with the reason.
 */
export function addImapMailbox(mailbox: MailboxInput): Promise<MailboxAccount> {
  return invoke<MailboxAccount>("add_imap_mailbox", { mailbox });
}
