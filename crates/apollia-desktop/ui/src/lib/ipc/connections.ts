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
  ConnectorEnrichmentView,
  McpConnectionTestResponse,
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
  config: Record<string, unknown>,
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

export function oauthStartFlow(
  provider: ProviderId,
  scopes: string[],
  sovereignty = "cloud_allowed",
): Promise<OauthStartResult> {
  return invoke<OauthStartResult>("oauth_start_flow", { provider, scopes, sovereignty });
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
