/**
 * Status helpers for the Connections surface.
 *
 * Pure, DOM-free, i18n-agnostic. Functions that surface user copy return an
 * i18n descriptor (`{ key, values }`) so the calling component resolves it with
 * the live `svelte-i18n` formatter, keeping this module unit-testable.
 */
import type { OauthClientIdStatus } from "$lib/ipc/oauthClients";
import type { McpHealth, McpServerStatusView } from "$lib/types";

/** Coarse operational state of a connection, driving dots and badges. */
export type ConnectionStatus = "active" | "attention" | "error" | "syncing" | "idle";

/** Badge variant matching a connection status (mirrors the `Badge` variants). */
export type StatusBadgeVariant = "success" | "warning" | "danger" | "neutral";

/** An i18n key plus optional interpolation values, resolved by the caller. */
export interface I18nDescriptor {
  key: string;
  values?: Record<string, number | string>;
}

/** Map operational health to a coarse connection status. */
export function statusFromHealth(health: McpHealth): ConnectionStatus {
  switch (health.state) {
    case "healthy":
      return "active";
    case "degraded":
    case "needs_reauth":
      return "attention";
    case "unavailable":
      return "error";
  }
}

/** Map an MCP server's operational health to a coarse connection status. */
export function statusOf(server: McpServerStatusView): ConnectionStatus {
  return statusFromHealth(server.health);
}

/** Accent colour token for a status dot / rail. */
export function statusAccent(status: ConnectionStatus): string {
  switch (status) {
    case "active":
      return "hsl(var(--success))";
    case "attention":
      return "hsl(var(--warning))";
    case "error":
      return "hsl(var(--destructive))";
    default:
      return "hsl(var(--muted-foreground))";
  }
}

/** Badge variant for a status. */
export function statusBadgeVariant(status: ConnectionStatus): StatusBadgeVariant {
  switch (status) {
    case "active":
      return "success";
    case "attention":
      return "warning";
    case "error":
      return "danger";
    default:
      return "neutral";
  }
}

/** i18n key for the short status label shown on a badge. */
export function statusLabelKey(status: ConnectionStatus): string {
  switch (status) {
    case "active":
      return "connections.badge_status_active";
    case "attention":
      return "connections.badge_status_attention";
    case "error":
      return "connections.badge_status_error";
    default:
      return "connections.badge_status_idle";
  }
}

/**
 * How complete a provider's OAuth credentials are.
 *
 * `partial` covers the case that used to fail after consent: Google issues a
 * client id and a paired secret, and its token endpoint rejects the exchange
 * when only the first is present.
 */
export type OauthReadiness = "ready" | "partial" | "none";

/**
 * Readiness of a provider's OAuth client credentials.
 *
 * Keys off `source` rather than the client id string, because an empty id with
 * a `builtin` source and an absent id are different states. The Picker API key
 * is deliberately excluded: it gates Drive picking, not connecting.
 *
 * Shared by Settings → Integrations and the Connections route so the two
 * cannot disagree about whether a connector is usable.
 */
export function oauthReadiness(status: OauthClientIdStatus): OauthReadiness {
  if (status.source === "none") return "none";
  if (status.requires_client_secret && !status.has_client_secret) return "partial";
  return "ready";
}

/** Whether a provider needs operator attention before a connect can succeed. */
export function needsOauthSetup(status: OauthClientIdStatus | undefined): boolean {
  return status === undefined || oauthReadiness(status) !== "ready";
}

/** Live-test verdict derived from health (excludes the transient idle/testing). */
export type McpTestVerdict = "ok" | "unverified" | "degraded" | "needs_reauth" | "error";

export interface TestOutcome {
  verdict: McpTestVerdict;
  detail?: string;
  error?: string;
}

/** Map a live-test health result to a verdict plus optional technical detail. */
export function testOutcomeFromHealth(health: McpHealth | null): TestOutcome {
  switch (health?.state) {
    case undefined:
    case "healthy":
      return { verdict: health?.state === "healthy" && !health.verified ? "unverified" : "ok" };
    case "degraded":
      return { verdict: "degraded", detail: `${health.category} - ${health.last_error}` };
    case "needs_reauth":
      return { verdict: "needs_reauth", detail: health.reason };
    case "unavailable":
      return { verdict: "error", error: health.reason };
  }
}

/** Relative "synced N ago" descriptor, or null when there is no last call. */
export function syncDescriptor(lastCallAt: string | null): I18nDescriptor | null {
  if (!lastCallAt) return null;
  const diffMs = Date.now() - new Date(lastCallAt).getTime();
  if (Number.isNaN(diffMs) || diffMs < 0) return null;
  const mins = Math.floor(diffMs / 60_000);
  if (mins < 1) return { key: "connections.sync_just_now" };
  if (mins < 60) return { key: "connections.sync_minutes_ago", values: { mins } };
  const hours = Math.floor(mins / 60);
  if (hours < 24) return { key: "connections.sync_hours_ago", values: { hours } };
  const days = Math.floor(hours / 24);
  return { key: "connections.sync_days_ago", values: { days } };
}

/** Sort order weight per status: error first, idle last. */
const STATUS_WEIGHT: Record<ConnectionStatus, number> = {
  error: 0,
  attention: 1,
  active: 2,
  syncing: 3,
  idle: 4,
};

/** Sort MCP servers by status severity, then name. Returns a new array. */
export function sortServers(servers: McpServerStatusView[]): McpServerStatusView[] {
  return [...servers].sort((a, b) => {
    const wa = STATUS_WEIGHT[statusOf(a)] ?? 99;
    const wb = STATUS_WEIGHT[statusOf(b)] ?? 99;
    if (wa !== wb) return wa - wb;
    return a.name.localeCompare(b.name);
  });
}
