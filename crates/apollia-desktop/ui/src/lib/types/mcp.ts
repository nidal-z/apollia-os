// ─── MCP integrations and registry ───

import type { ErrorCategory } from "./reasoning";

export interface McpServerStatusView {
  name: string;
  server_info: string;
  tools_count: number;
  requires_approval: boolean;
  connected: boolean;
  pid: number | null;
  uptime_secs: number | null;
  last_call_at: string | null;
  error: string | null;
  package: string | null;
  transport: string;
  /** Operational health, orthogonal to `connected`. Drives the status badge. */
  health: McpHealth;
}

/**
 * Operational health of an MCP server, mirrored from `apollia_core::McpHealth`.
 *
 * Discriminated by `state`. `connected` (process alive) and `health` are
 * orthogonal: a server can be connected yet `degraded` or `needs_reauth`.
 */
export type McpHealth =
  | { state: "healthy"; verified: boolean }
  | {
      state: "degraded";
      category: ErrorCategory;
      last_error: string;
      consecutive_failures: number;
      since: string;
    }
  | { state: "needs_reauth"; reason: string }
  | { state: "unavailable"; reason: string };

/** UI severity bucket for {@link McpHealth}. */
export type McpHealthSeverity = "ok" | "warn" | "reauth" | "error";

/** MCP server detail including its tool list, returned by `get_mcp_server_detail`. */
export interface McpServerDetailView {
  status: McpServerStatusView;
  tools: McpToolSummaryView[];
  config: McpServerConfigView;
}

/** Summary of a single tool exposed by an MCP server. */
export interface McpToolSummaryView {
  full_name: string;
  local_name: string;
  description: string | null;
  input_schema: Record<string, unknown>;
}

/** Read-only configuration of an MCP server with secret values redacted. */
export interface McpServerConfigView {
  name: string;
  command: string;
  args: string[];
  env_keys: string[];
  transport: string;
  requires_approval: boolean;
  tags: string[];
}

/** Trust level badge shown next to a connector in the catalogue. */
export type TrustLevel =
  | "verified_official"
  | "community_verified"
  | "community"
  | "custom";

/** A remote connection endpoint for an MCP registry server. */
export interface RegistryRemoteView {
  /** Transport type: `"streamable-http"` or `"sse"`. */
  type: string;
  /** Connection URL (e.g. `"https://mcp.notion.com/mcp"`). */
  url: string;
  /** HTTP headers required to authenticate or configure the connection. */
  headers: RegistryRemoteHeaderView[];
}

/** A required HTTP header for a remote MCP connection. */
export interface RegistryRemoteHeaderView {
  name: string;
  description: string | null;
  isRequired: boolean;
  isSecret: boolean;
}

/** MCP server entry from the registry catalogue, enriched with trust and install state. */
export interface RegistryServerView {
  name: string;
  title: string | null;
  description: string | null;
  version: string;
  repository_url: string | null;
  website_url: string | null;
  packages: RegistryPackageView[] | null;
  trust_level: TrustLevel;
  category: string | null;
  enrichment: ConnectorEnrichmentView | null;
  is_installed: boolean;
  /** Remote connection endpoints (streamable-http, SSE). Empty for package-only servers. */
  remotes: RegistryRemoteView[];
}

/**
 * An installable package for an MCP registry server.
 *
 * Field names mirror the wire format produced by serde in
 * `apollia-desktop::mcp::registry_client::RegistryPackage` (camelCase for the
 * renamed fields, matching the upstream MCP registry JSON schema).
 */
export interface RegistryPackageView {
  registryType: string;
  identifier: string;
  version: string;
  runtimeHint: string | null;
  /** Transport block (`{ "type": "stdio" }`), serialised by `RegistryTransport`. */
  transport: { type: string };
  environmentVariables: RegistryEnvVarView[];
  packageArguments: RegistryPackageArgView[];
}

/** An environment variable required by an MCP server package. */
export interface RegistryEnvVarView {
  name: string;
  description: string | null;
  isRequired: boolean;
  isSecret: boolean;
}

/**
 * A positional or named argument forwarded to an MCP package at launch.
 * Field names mirror the wire format produced by serde (`#[serde(rename = ...)]`
 * in `apollia-desktop::mcp::registry_client::RegistryPackageArg`).
 */
export interface RegistryPackageArgView {
  /** Argument kind: `"positional"` or `"named"`. Wire key is `type`. */
  type: string;
  /** Fixed value injected verbatim when present; null means the user must supply it. */
  value: string | null;
  valueHint: string | null;
  description: string | null;
  isRequired: boolean;
  /** True when the user input is split on whitespace and one argv entry is emitted per token. */
  isRepeatable: boolean;
}

/** UI-friendly metadata enriching a registry server entry for the catalogue. */
export interface ConnectorEnrichmentView {
  operator_label: string;
  category: string;
  icon_name: string;
  trust_level: TrustLevel;
  auth_help_url: string | null;
  auth_help_text: string | null;
  /** Optional i18n key resolved by the wizard via `$t(key)`. When set, it
   *  takes precedence over `auth_help_text` and lets us ship long localised
   *  explanations in the bundled FR/EN catalogs instead of inline strings. */
  auth_help_i18n_key: string | null;
  default_requires_approval: boolean;
  /** Env var name carrying a pre-registered OAuth client id for AS that
   *  don't support CIMD or anonymous DCR (Figma). The wizard resolves the
   *  env var via `mcp_oauth_resolve_client_id` and passes the value to
   *  `mcp_oauth_login`. */
  oauth_pre_registered_client_id_env: string | null;
}

/** Result of testing an MCP server connection without persisting a session. */
export interface McpConnectionTestResultView {
  server_info: string;
  protocol_version: string;
  tools: McpToolSummaryView[];
  test_duration_ms: number;
  /**
   * Operational health of the live session this test targeted. `null` for a
   * pre-install wizard test. `degraded` / `needs_reauth` mean the handshake is
   * reachable but real operations are not succeeding: do not report a plain OK.
   */
  live_health?: McpHealth | null;
}

/**
 * Tagged envelope returned by `test_mcp_connection`.
 *
 * The wizard dispatches its Auth step UI on `kind`:
 * - `success` → list tools, allow continue.
 * - `oauth_required` → switch to "Sign in with <provider>" mode and call
 *   `mcp_oauth_login` once the user clicks.
 */
export type McpConnectionTestResponse =
  | ({ kind: "success" } & McpConnectionTestResultView)
  | { kind: "oauth_required"; www_authenticate: string };

/** Discovery payload returned by `mcp_oauth_discover`. */
export interface McpOAuthDiscoveryResult {
  as_url: string;
  scopes_supported: string[];
  scope_descriptions: Record<string, string>;
  registration_supported: boolean;
}

/** Sign-in outcome returned by `mcp_oauth_login`. */
export interface McpOAuthAccount {
  sub: string | null;
  email: string | null;
  scopes: string[];
}

/** Input payload for adding a new MCP server via `add_mcp_server`. */
export interface McpServerConfigInput {
  name: string;
  /** Executable to spawn. Omitted for network-based transports (`streamable-http`, `sse`). */
  command?: string;
  /** Arguments forwarded to the command. Omitted for network-based transports. */
  args?: string[];
  /** Remote server URL. Required for `streamable-http` and `sse` transports. */
  url?: string;
  env: Record<string, string>;
  transport: string;
  requires_approval: boolean;
  tags: string[];
}
