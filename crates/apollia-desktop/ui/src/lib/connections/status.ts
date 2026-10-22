/**
 * Status helpers for the Connections surface.
 *
 * Pure, DOM-free, i18n-agnostic. Functions that surface user copy return an
 * i18n descriptor (`{ key, values }`) so the calling component resolves it with
 * the live `svelte-i18n` formatter, keeping this module unit-testable.
 */
import type { OauthClientIdStatus } from "$lib/ipc/oauthClients";
import type { McpHealth } from "$lib/types";

/** An i18n key plus optional interpolation values, resolved by the caller. */
export interface I18nDescriptor {
  key: string;
  values?: Record<string, number | string>;
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
