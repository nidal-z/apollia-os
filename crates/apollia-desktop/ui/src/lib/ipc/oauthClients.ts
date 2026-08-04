/**
 * Typed Tauri command wrappers for the Settings > Integrations surface.
 *
 * This surface manages the OAuth *client credentials* (client_id,
 * client_secret, Picker API key) of the Google and Microsoft connectors, which
 * is distinct from the Connections surface that lists connected accounts.
 *
 * Every `invoke()` used by the Integrations route and its sub-components goes
 * through this module. Wrapper exports are camelCase; the underlying Rust
 * command names are snake_case. Secrets are write-only: they are sent to Rust
 * once and never echoed back, so no getter for a secret value exists here.
 * Error handling stays at the call site.
 */
import { invoke } from "@tauri-apps/api/core";

/** Resolution source of a resolved credential value. */
export type OauthSource = "env" | "file" | "builtin" | "none";

/**
 * Snapshot of the active OAuth client_id per connector provider plus the
 * source of that value and the write-only state of the secret / API key.
 * Secret values are never present; only their presence (`has_*`) and source.
 */
export interface OauthClientIdStatus {
  provider: string;
  effective_client_id: string;
  source: OauthSource;
  override_client_id: string | null;
  has_client_secret: boolean;
  client_secret_source: OauthSource;
  requires_client_secret: boolean;
  has_api_key: boolean;
  api_key_source: OauthSource;
  requires_api_key: boolean;
}

/**
 * Drive folder configuration loaded from `~/.apollia/drive-prefs.toml`. Only
 * Google exposes it today; keyed by connected account_id.
 */
export interface DriveFolderStatus {
  account_id: string;
  folder_path: string | null;
  effective_folder_path: string;
}

/**
 * Outcome of a non-interactive OAuth client credential check: whether the
 * configuration looks usable (`ok`) plus a human-readable `detail`. This is a
 * static validation (client_id present + well-formed, secret present for Google,
 * provider OIDC discovery reachable), NOT a full interactive OAuth round-trip.
 */
export interface OauthClientTestResult {
  ok: boolean;
  detail: string;
}

/** List the OAuth client_id status of every connector provider. */
export function listOauthClientIds(): Promise<OauthClientIdStatus[]> {
  return invoke<OauthClientIdStatus[]>("oauth_list_client_ids");
}

/**
 * Validate a provider's OAuth client configuration without an interactive
 * sign-in: checks credential presence/shape and that the provider's OIDC
 * discovery endpoint is reachable. `provider` is `"google"` or `"microsoft"`.
 */
export function oauthTestClient(
  provider: string,
): Promise<OauthClientTestResult> {
  return invoke<OauthClientTestResult>("oauth_test_client", { provider });
}

/** Persist (or clear, when empty) the client_id override for a provider. */
export function setOauthClientId(
  provider: string,
  clientId: string,
): Promise<void> {
  return invoke("oauth_set_client_id", { provider, clientId });
}

/** Persist (or clear, when empty) the client_secret for a provider. */
export function setOauthClientSecret(
  provider: string,
  clientSecret: string,
): Promise<void> {
  return invoke("oauth_set_client_secret", { provider, clientSecret });
}

/**
 * Import a provider's OAuth client from the JSON file its console produces.
 *
 * Google only: the Cloud console hands out a downloadable credentials file, so
 * reading it directly spares the operator from picking `client_id` and
 * `client_secret` out of it by hand. Writes both into
 * `~/.apollia/oauth-clients.toml`.
 */
export function importOauthClientJson(provider: string, path: string): Promise<void> {
  return invoke("oauth_import_client_json", { provider, path });
}

/** Persist (or clear, when empty) the Picker API key for a provider. */
export function setOauthApiKey(
  provider: string,
  apiKey: string,
): Promise<void> {
  return invoke("oauth_set_api_key", { provider, apiKey });
}

/** List the per-account Drive folder configuration (Google only today). */
export function listDriveFolders(): Promise<DriveFolderStatus[]> {
  return invoke<DriveFolderStatus[]>("oauth_list_drive_folders");
}

/** Persist the Drive root folder for an account (empty = Drive root). */
export function setDriveFolder(
  accountId: string,
  folderPath: string,
): Promise<void> {
  return invoke("oauth_set_drive_folder", { accountId, folderPath });
}

/** Reset the Drive root folder for an account back to the default. */
export function resetDriveFolder(accountId: string): Promise<void> {
  return invoke("oauth_reset_drive_folder", { accountId });
}
