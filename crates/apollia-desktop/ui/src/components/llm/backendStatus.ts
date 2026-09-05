/**
 * What an LLM backend card can honestly say about a backend.
 *
 * The card used to read `!enabled || last_ping_error` as its whole truth and
 * paint everything else green ("Running"). That collapses two different facts:
 * a ping that came back clean, and no ping at all. `last_ping_error` is filled
 * from the desktop `LlmPingCache`, which lives in memory for one app run, so a
 * backend nobody has tested since launch carries no error and used to read as
 * running while nothing had contacted it.
 *
 * The route that lists backends (`routes/Llm.svelte`) does not ping on mount;
 * only the settings route does. Splitting `untested` out is what keeps the two
 * surfaces from disagreeing about what green means.
 *
 * Pure and i18n-agnostic: labels are catalogue keys resolved by the caller.
 */
import type { LlmBackendConfig } from "$lib/types";

/** Coarse state of a backend as far as the desktop can measure it. */
export type BackendStatus = "error" | "reachable" | "untested";

/** Badge tone matching a backend status (a subset of the `Badge` variants). */
export type BackendStatusVariant = "danger" | "success" | "neutral";

/**
 * Derive the status of a backend.
 *
 * A disabled backend stays in the error bucket, as the card has always shown
 * it. `reachable` requires a recorded ping: the absence of an error is not a
 * measurement.
 */
export function backendStatus(backend: LlmBackendConfig): BackendStatus {
  if (!backend.enabled || backend.last_ping_error) {
    return "error";
  }
  return backend.last_ping_at ? "reachable" : "untested";
}

/** Catalogue key for the operator-facing status line. */
export function backendStatusLabelKey(status: BackendStatus): string {
  switch (status) {
    case "error":
      return "llm.status_error";
    case "reachable":
      return "llm.running";
    case "untested":
      return "llm.status_untested";
  }
}

/** Catalogue key for the short builder badge. */
export function backendStatusBadgeKey(status: BackendStatus): string {
  switch (status) {
    case "error":
      return "common.status.error";
    case "reachable":
      return "common.status.ready";
    case "untested":
      return "common.status.untested";
  }
}

/** Badge tone for a status. */
export function backendStatusBadgeVariant(status: BackendStatus): BackendStatusVariant {
  switch (status) {
    case "error":
      return "danger";
    case "reachable":
      return "success";
    case "untested":
      return "neutral";
  }
}

/** Status-dot colour as a token reference. */
export function backendStatusDotColor(status: BackendStatus): string {
  switch (status) {
    case "error":
      return "hsl(var(--destructive))";
    case "reachable":
      return "hsl(var(--success))";
    case "untested":
      return "hsl(var(--muted-foreground))";
  }
}
