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

export interface OauthAccountInfo {
  provider: ProviderId;
  account_id: string;
}

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

export function setMcpServerApproval(
  name: string,
  requiresApproval: boolean,
): Promise<void> {
  return invoke<void>("set_mcp_server_approval", { name, requiresApproval });
}

export function testMcpLiveServer(name: string): Promise<McpConnectionTestResponse> {
  return invoke<McpConnectionTestResponse>("test_mcp_live_server", { name });
}

export function restartMcpServer(name: string): Promise<McpServerStatusView> {
  return invoke<McpServerStatusView>("restart_mcp_server", { name });
}

export function deleteMcpSecret(serverName: string, envVar: string): Promise<void> {
  return invoke<void>("delete_mcp_secret", { serverName, envVar });
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

export function oauthGetStatus(): Promise<OauthAccountInfo[]> {
  return invoke<OauthAccountInfo[]>("oauth_get_status");
}

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

export function oauthDisconnect(
  provider: ProviderId,
  accountId: string,
): Promise<void> {
  return invoke<void>("oauth_disconnect", { provider, accountId });
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
  requires_approval: boolean;
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
