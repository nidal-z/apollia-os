/**
 * Error formatting for the Connections surface.
 *
 * i18n-agnostic: the caller passes a translator so these helpers stay pure.
 * Specific Tauri error kinds get a dedicated operator-friendly message; the
 * generic path preserves the raw text.
 */
import type { ErrorTranslate } from "$lib/errors/humanize";

interface TauriErrorShape {
  kind?: string;
  detail?: string;
  message?: string;
}

/** Whether a raw error is the "OAuth client not configured" Tauri kind. */
export function isMissingClientError(e: unknown): boolean {
  return (e as TauriErrorShape | null)?.kind === "oauth_client_not_configured";
}

/**
 * Whether a raw error is the "OAuth client secret missing" Tauri kind.
 *
 * Distinct from a missing client id: the operator has already been to the
 * provider console and pasted something, so the copy has to name the half
 * that is absent rather than restart them from scratch.
 */
export function isMissingSecretError(e: unknown): boolean {
  return (e as TauriErrorShape | null)?.kind === "oauth_client_secret_missing";
}

/**
 * True when the connection admission refused the flow: the organisation
 * policy forbids it. The dialog says so in its own box rather than as an
 * error, since retrying cannot change the outcome.
 */
export function isRefusedByPolicy(e: unknown): boolean {
  return (e as TauriErrorShape | null)?.kind === "refused_by_policy";
}

/**
 * What every refusal of the organisation policy starts with, as the runtime
 * words it. A command that relays the runtime's refusal as text (adding an
 * MCP server) is recognised by it.
 */
const POLICY_REFUSAL_PREFIX = "refused by the organisation policy";

/** True when a raw error, structured or plain text, is a policy refusal. */
export function isPolicyRefusal(e: unknown): boolean {
  if (isRefusedByPolicy(e)) return true;
  const text =
    typeof e === "string"
      ? e
      : e instanceof Error
        ? e.message
        : ((e as TauriErrorShape | null)?.message ?? (e as TauriErrorShape | null)?.detail);
  return typeof text === "string" && text.toLowerCase().includes(POLICY_REFUSAL_PREFIX);
}

/**
 * Format a raw Tauri error into a display string. Known kinds map to localized
 * copy; everything else falls back to the raw detail / message / string form.
 */
export function formatTauriError(e: unknown, translate: ErrorTranslate): string {
  if (isPolicyRefusal(e)) return translate("connections.error_refused_by_policy");
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  const err = (e ?? {}) as TauriErrorShape;
  if (err.kind === "oauth_client_not_configured") {
    return translate("connections.error_oauth_client_missing");
  }
  if (err.kind === "oauth_client_secret_missing") {
    return translate("connections.error_oauth_client_secret_missing");
  }
  if (err.kind === "invalid_client_file") {
    // The detail names the offending field or parse position, which is the
    // only actionable part, so it is appended rather than swallowed.
    const reason = err.detail ?? "";
    const label = translate("connections.error_invalid_client_file");
    return reason ? `${label} ${reason}` : label;
  }
  if (err.kind === "service" && err.detail) {
    // The daemon this window is attached to already wrote the sentence.
    return err.detail;
  }
  if (err.kind && err.detail) return `${err.kind}: ${err.detail}`;
  if (err.message) return err.message;
  if (err.kind) return err.kind;
  return String(e);
}

/** Extract a plain message from an unknown thrown value. */
export function messageOf(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}
