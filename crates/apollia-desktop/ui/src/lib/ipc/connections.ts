/**
 * Typed Tauri command wrappers for the Connections surface.
 *
 * Every `invoke()` used by the Connections route and its sub-components goes
 * through this module. Wrapper exports are camelCase; the underlying Rust
 * command names are snake_case. Error handling stays at the call site.
 */
import { invoke } from "@tauri-apps/api/core";
import { getProfile } from "$lib/ipc/profile";
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

/** The sovereignty profile as the Rust `SovereigntyProfile` enum spells it. */
export type SovereigntyProfile = "local_only" | "cloud_allowed";

/**
 * Read the user's sovereignty setting and map it to the backend enum.
 *
 * The profile stores `local-only` / `local-preferred` / `cloud-ok`; the command
 * takes `local_only` / `cloud_allowed`. Only `local-only` blocks: the
 * `local-preferred` profile allows cloud as a last resort after approval, which
 * is what an OAuth connection is.
 *
 * The permitting values are named explicitly rather than the blocking one, so
 * a value nobody anticipated blocks instead of passing. `constraints.sovereignty`
 * is a required tier-1 profile key marked sensitive and carries no default: a
 * profile reaching this point without it is not initialised, and an absent
 * sensitive setting must not read as consent. An unreadable store blocks for the
 * same reason.
 */
const CLOUD_PERMITTING = new Set(["cloud-ok", "local-preferred"]);

export async function resolveSovereignty(): Promise<SovereigntyProfile> {
  try {
    const profile = await getProfile();
    const entry = profile.entries.find((e) => e.key === "constraints.sovereignty");
    return entry && CLOUD_PERMITTING.has(entry.value) ? "cloud_allowed" : "local_only";
  } catch {
    return "local_only";
  }
}

/**
 * Start a cloud OAuth flow.
 *
 * `sovereignty` is required and carries no default: the command refuses the
 * flow when it is `local_only`, and a default here would let a caller bypass
 * that gate by omission, which is exactly what happened while the parameter
 * defaulted to `cloud_allowed`. Pass {@link resolveSovereignty}.
 */
export function oauthStartFlow(
  provider: ProviderId,
  scopes: string[],
  sovereignty: SovereigntyProfile,
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
