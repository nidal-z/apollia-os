import { vi, describe, test, expect, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue(undefined),
}));

import { invoke } from "@tauri-apps/api/core";
import type { OauthClientIdStatus } from "$lib/ipc/oauthClients";
import { oauthSetClientSecret } from "$lib/ipc/oauthClients";
import {
  GOOGLE_REDIRECT_URI,
  clearApiKey,
  clearClientSecret,
  clientIdOriginKey,
  copyRedirect,
  driveFolderFor,
  importClientJson,
  listDriveFolders,
  oauthClientFields,
  reloadClient,
  resetDriveFolder,
  saveApiKey,
  saveClientId,
  saveClientSecret,
  saveDriveFolder,
  testClient,
} from "./oauthClient";

const mockedInvoke = vi.mocked(invoke);

function status(overrides: Partial<OauthClientIdStatus>): OauthClientIdStatus {
  return {
    provider: "google",
    effective_client_id: "",
    source: "none",
    override_client_id: null,
    has_client_secret: false,
    client_secret_source: "none",
    requires_client_secret: true,
    has_api_key: false,
    api_key_source: "none",
    requires_api_key: true,
    ...overrides,
  };
}

/** The single Tauri call the last gesture made. */
function lastCall(): [string, Record<string, unknown> | undefined] {
  const calls = mockedInvoke.mock.calls;
  return calls[calls.length - 1] as [string, Record<string, unknown> | undefined];
}

beforeEach(() => {
  vi.clearAllMocks();
  mockedInvoke.mockResolvedValue(undefined);
});

describe("oauthClientFields - what the page of a connection offers", () => {
  test("Google offers every field, the secret required", () => {
    // GIVEN the Google client
    const google = status({ provider: "google" });
    // WHEN the page decides its fields
    const fields = oauthClientFields(google);
    // THEN secret, API key, import, redirect and Drive folders are all there
    expect(fields).toEqual({
      secret: true,
      secretRequired: true,
      apiKey: true,
      importJson: true,
      redirect: true,
      driveFolders: true,
    });
  });

  test("Microsoft with the client Apollia ships offers the id only", () => {
    // GIVEN Microsoft resolving the built-in public client
    const ms = status({
      provider: "microsoft",
      source: "builtin",
      requires_client_secret: false,
      requires_api_key: false,
    });
    // WHEN the page decides its fields
    const fields = oauthClientFields(ms);
    // THEN no secret, no key, no import, no redirect, no Drive
    expect(fields.secret).toBe(false);
    expect(fields.apiKey || fields.importJson || fields.redirect || fields.driveFolders).toBe(
      false,
    );
  });

  test("Microsoft with the user's own client offers an optional secret", () => {
    // GIVEN Microsoft resolving a client id the user saved
    const ms = status({
      provider: "microsoft",
      source: "file",
      requires_client_secret: false,
      requires_api_key: false,
    });
    // WHEN the page decides its fields
    const fields = oauthClientFields(ms);
    // THEN the secret row is offered, and not required
    expect(fields.secret).toBe(true);
    expect(fields.secretRequired).toBe(false);
  });
});

describe("the gestures of the OAuth client, one Tauri command each", () => {
  test("saving the client id writes it, trimmed, through oauth_set_client_id", async () => {
    // GIVEN a pasted id with surrounding blanks
    // WHEN it is saved
    await saveClientId("google", " 1-a.apps.googleusercontent.com ");
    // THEN the command and the trimmed value reach Tauri
    expect(lastCall()).toEqual([
      "oauth_set_client_id",
      { provider: "google", clientId: "1-a.apps.googleusercontent.com" },
    ]);
  });

  test("saving and clearing the secret go through oauth_set_client_secret", async () => {
    // GIVEN a secret typed on the page
    // WHEN it is saved, then cleared
    await saveClientSecret("google", "GOCSPX-x ");
    const saved = lastCall();
    await clearClientSecret("google");
    const cleared = lastCall();
    // THEN both use the command the settings section uses, an empty value clearing
    expect(saved).toEqual([
      "oauth_set_client_secret",
      { provider: "google", clientSecret: "GOCSPX-x" },
    ]);
    expect(cleared).toEqual(["oauth_set_client_secret", { provider: "google", clientSecret: "" }]);
    await oauthSetClientSecret("google", "GOCSPX-x");
    expect(lastCall()).toEqual(saved);
  });

  test("saving and clearing the API key go through oauth_set_api_key", async () => {
    // GIVEN an API key typed on the page
    // WHEN it is saved, then cleared
    await saveApiKey("google", "AIza-k");
    const saved = lastCall();
    await clearApiKey("google");
    // THEN both reach oauth_set_api_key
    expect(saved).toEqual(["oauth_set_api_key", { provider: "google", apiKey: "AIza-k" }]);
    expect(lastCall()).toEqual(["oauth_set_api_key", { provider: "google", apiKey: "" }]);
  });

  test("importing the client JSON sends the picked file, and a dismissed picker sends nothing", async () => {
    // GIVEN a file picked, then a picker dismissed
    // WHEN each is imported
    const imported = await importClientJson("google", "/tmp/client.json");
    const callsAfterImport = mockedInvoke.mock.calls.length;
    const dismissed = await importClientJson("google", null);
    // THEN the file goes through oauth_import_client_json, the dismissal calls nothing
    expect(imported).toBe(true);
    expect(mockedInvoke.mock.calls[0]).toEqual([
      "oauth_import_client_json",
      { provider: "google", path: "/tmp/client.json" },
    ]);
    expect(dismissed).toBe(false);
    expect(mockedInvoke.mock.calls.length).toBe(callsAfterImport);
  });

  test("testing the client asks oauth_test_client and returns its verdict", async () => {
    // GIVEN the runtime answering a verdict
    mockedInvoke.mockResolvedValueOnce({ ok: true, detail: "reachable" });
    // WHEN the client is tested
    const verdict = await testClient("google");
    // THEN the verdict comes back as is
    expect(lastCall()).toEqual(["oauth_test_client", { provider: "google" }]);
    expect(verdict).toEqual({ ok: true, detail: "reachable" });
  });

  test("retrying reads the clients again and keeps the provider's row", async () => {
    // GIVEN the runtime listing both providers
    mockedInvoke.mockResolvedValueOnce([
      status({ provider: "google", source: "file" }),
      status({ provider: "microsoft", source: "builtin" }),
    ]);
    // WHEN the Google page retries
    const row = await reloadClient("google");
    // THEN it lists the clients and keeps the Google row
    expect(lastCall()[0]).toBe("oauth_list_client_ids");
    expect(row?.provider).toBe("google");
    expect(row?.source).toBe("file");
  });

  test("copying the redirect address puts the Google loopback on the clipboard", async () => {
    // GIVEN a clipboard that accepts, then one that refuses
    const writeText = vi.fn().mockResolvedValue(undefined);
    const refusing = { writeText: vi.fn().mockRejectedValue(new Error("denied")) };
    // WHEN the address is copied to each
    const ok = await copyRedirect({ writeText });
    const refused = await copyRedirect(refusing);
    // THEN the loopback is written, and a refusal reads as a failed copy
    expect(writeText).toHaveBeenCalledWith(GOOGLE_REDIRECT_URI);
    expect(ok).toBe(true);
    expect(refused).toBe(false);
  });

  test("the Drive folder of an account is listed, changed and brought back to the default", async () => {
    // GIVEN the runtime listing one account's folder
    const folders = [
      { account_id: "a@example.com", folder_path: null, effective_folder_path: "Apollia" },
    ];
    mockedInvoke.mockResolvedValueOnce(folders);
    // WHEN the folders are listed, one is changed, then reset
    const listed = await listDriveFolders();
    const listCall = lastCall();
    await saveDriveFolder("a@example.com", " Work/AI ");
    const saveCall = lastCall();
    await resetDriveFolder("a@example.com");
    // THEN each gesture reaches its command, and the account finds its folder
    expect(listCall[0]).toBe("oauth_list_drive_folders");
    expect(driveFolderFor(listed, "a@example.com")?.effective_folder_path).toBe("Apollia");
    expect(saveCall).toEqual([
      "oauth_set_drive_folder",
      { accountId: "a@example.com", folderPath: "Work/AI" },
    ]);
    expect(lastCall()).toEqual(["oauth_reset_drive_folder", { accountId: "a@example.com" }]);
  });
});

describe("clientIdOriginKey - where the client id comes from", () => {
  test("each source names its own origin", () => {
    // GIVEN the sources a client id can come from
    const sources = ["env", "file", "builtin", "none"] as const;
    // WHEN each is turned into its key
    const keys = sources.map(clientIdOriginKey);
    // THEN each lands under the connection's origin namespace
    expect(keys).toEqual([
      "connections.oauth_client.origin.env",
      "connections.oauth_client.origin.file",
      "connections.oauth_client.origin.builtin",
      "connections.oauth_client.origin.none",
    ]);
  });
});
