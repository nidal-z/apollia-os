/**
 * The OAuth client of a native connection, as its page in Connections shows
 * and edits it: which fields the page offers, where each value comes from,
 * and one function per gesture.
 *
 * Every gesture goes through the same typed wrappers as the settings section,
 * so the page and the settings write through the same Tauri commands: the
 * client id into `oauth-clients.toml`, the client secret and the API key into
 * the secret store.
 */
import type { ProviderId } from "$lib/connections/capabilities";
import {
  oauthImportClientJson,
  oauthListClientIds,
  oauthListDriveFolders,
  oauthResetDriveFolder,
  oauthSetApiKey,
  oauthSetClientId,
  oauthSetClientSecret,
  oauthSetDriveFolder,
  oauthTestClient,
  type DriveFolderStatus,
  type OauthClientIdStatus,
  type OauthClientTestResult,
  type OauthSource,
} from "$lib/ipc/oauthClients";

/** The loopback redirect a Google client must allow. */
export const GOOGLE_REDIRECT_URI = "http://127.0.0.1/";

/** What the page of a connection offers for its OAuth client. */
export interface OauthClientFields {
  /** The client secret row. */
  secret: boolean;
  /** Whether the client secret is required (Google) or optional. */
  secretRequired: boolean;
  /** The API key section (Google Picker). */
  apiKey: boolean;
  /** "Import the client JSON": Google's console exports a file. */
  importJson: boolean;
  /** "Copy the redirect address": only a Google client declares one. */
  redirect: boolean;
  /** The Drive folder of each connected account. */
  driveFolders: boolean;
}

/**
 * The fields of `status`'s page. Microsoft carries no secret with the client
 * Apollia ships; the row appears once the user supplies a client of their own
 * (an id saved here, or a secret already stored).
 */
export function oauthClientFields(status: OauthClientIdStatus): OauthClientFields {
  const google = status.provider === "google";
  const userClient = status.source === "file" || status.has_client_secret;
  return {
    secret: status.requires_client_secret || userClient,
    secretRequired: status.requires_client_secret,
    apiKey: status.requires_api_key,
    importJson: google,
    redirect: google,
    driveFolders: google,
  };
}

/** The i18n key naming where the client id comes from. */
export function clientIdOriginKey(source: OauthSource): string {
  return `connections.oauth_client.origin.${source}`;
}

/** The status row of `provider`, or `undefined` when the list does not carry it. */
export function statusFor(
  statuses: OauthClientIdStatus[],
  provider: ProviderId,
): OauthClientIdStatus | undefined {
  return statuses.find((s) => s.provider === provider);
}

/** Retry: read the client of `provider` again. */
export async function reloadClient(
  provider: ProviderId,
): Promise<OauthClientIdStatus | undefined> {
  return statusFor(await oauthListClientIds(), provider);
}

/** Save the client id (an empty value clears it). */
export function saveClientId(provider: ProviderId, value: string): Promise<void> {
  return oauthSetClientId(provider, value.trim());
}

/** Save the client secret, into the secret store. */
export function saveClientSecret(provider: ProviderId, value: string): Promise<void> {
  return oauthSetClientSecret(provider, value.trim());
}

/** Remove the stored client secret. */
export function clearClientSecret(provider: ProviderId): Promise<void> {
  return oauthSetClientSecret(provider, "");
}

/** Save the Google API key, into the secret store. */
export function saveApiKey(provider: ProviderId, value: string): Promise<void> {
  return oauthSetApiKey(provider, value.trim());
}

/** Remove the stored API key. */
export function clearApiKey(provider: ProviderId): Promise<void> {
  return oauthSetApiKey(provider, "");
}

/**
 * Import the client JSON file at `path`. `null` (the picker was dismissed)
 * imports nothing and answers `false`.
 */
export async function importClientJson(
  provider: ProviderId,
  path: string | null,
): Promise<boolean> {
  if (path === null) return false;
  await oauthImportClientJson(provider, path);
  return true;
}

/** Test the client without a sign-in. */
export function testClient(provider: ProviderId): Promise<OauthClientTestResult> {
  return oauthTestClient(provider);
}

/** Copy the redirect address to the clipboard; `false` when the copy failed. */
export async function copyRedirect(
  clipboard: Pick<Clipboard, "writeText">,
  uri: string = GOOGLE_REDIRECT_URI,
): Promise<boolean> {
  try {
    await clipboard.writeText(uri);
    return true;
  } catch {
    return false;
  }
}

/** The Drive folder of every connected Google account. */
export function listDriveFolders(): Promise<DriveFolderStatus[]> {
  return oauthListDriveFolders();
}

/** The Drive folder of `accountId` in `folders`, if listed. */
export function driveFolderFor(
  folders: DriveFolderStatus[],
  accountId: string,
): DriveFolderStatus | undefined {
  return folders.find((f) => f.account_id === accountId);
}

/** Change the Drive folder of `accountId` (empty = the Drive root). */
export function saveDriveFolder(accountId: string, path: string): Promise<void> {
  return oauthSetDriveFolder(accountId, path.trim());
}

/** Bring the Drive folder of `accountId` back to the default. */
export function resetDriveFolder(accountId: string): Promise<void> {
  return oauthResetDriveFolder(accountId);
}
