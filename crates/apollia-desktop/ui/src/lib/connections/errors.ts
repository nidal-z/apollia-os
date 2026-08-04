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
 * Format a raw Tauri error into a display string. Known kinds map to localized
 * copy; everything else falls back to the raw detail / message / string form.
 */
export function formatTauriError(e: unknown, translate: ErrorTranslate): string {
  if (typeof e === "string") return e;
  const err = (e ?? {}) as TauriErrorShape;
  if (err.kind === "sovereignty_blocked") {
    return translate("connections.error_sovereignty_blocked");
  }
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
  if (err.kind && err.detail) return `${err.kind}: ${err.detail}`;
  if (err.message) return err.message;
  if (err.kind) return err.kind;
  return String(e);
}

/** Extract a plain message from an unknown thrown value. */
export function messageOf(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}
