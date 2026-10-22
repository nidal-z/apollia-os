/**
 * The native connectors (Google, Microsoft): their identifier, and the keys of
 * what an account of each gives. Pure data: every label is an i18n key
 * resolved by the caller.
 */

/** Native connector provider identifier. */
export type ProviderId = "google" | "microsoft";

/** The keys of what an account of each provider gives its agents, one per service. */
export function scopeLabelKeys(providerId: ProviderId): string[] {
  return providerId === "google"
    ? [
        "connections.settings_scope_gmail",
        "connections.settings_scope_gcal",
        "connections.settings_scope_gdrive",
      ]
    : [
        "connections.settings_scope_outlook_mail",
        "connections.settings_scope_outlook_cal",
        "connections.settings_scope_onedrive",
      ];
}
