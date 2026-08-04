import { describe, it, expect } from "vitest";
import { oauthReadiness, needsOauthSetup } from "./status";
import type { OauthClientIdStatus } from "$lib/ipc/oauthClients";

/** A credential status row, with the shape `oauth_list_client_ids` returns. */
function statusOf(over: Partial<OauthClientIdStatus> = {}): OauthClientIdStatus {
  return {
    provider: "google",
    effective_client_id: "123-abc.apps.googleusercontent.com",
    source: "file",
    override_client_id: null,
    has_client_secret: true,
    client_secret_source: "file",
    requires_client_secret: true,
    has_api_key: false,
    api_key_source: "none",
    requires_api_key: true,
    ...over,
  };
}

describe("oauthReadiness", () => {
  it("reports none on a fresh install, where no source resolves", () => {
    // GIVEN a provider whose credentials resolve from nowhere, which on a
    // fresh install is Google's state and no longer Microsoft's
    const status = statusOf({ source: "none", effective_client_id: "" });

    // WHEN readiness is derived
    // THEN the surface can say "setup required" rather than "not connected"
    expect(oauthReadiness(status)).toBe("none");
    expect(needsOauthSetup(status)).toBe(true);
  });

  it("reports partial when Google has a client id but no secret", () => {
    // GIVEN the half-configured state that used to fail only after consent
    const status = statusOf({ requires_client_secret: true, has_client_secret: false });

    // WHEN readiness is derived
    // THEN it is not ready, and connecting is refused before the browser opens
    expect(oauthReadiness(status)).toBe("partial");
    expect(needsOauthSetup(status)).toBe(true);
  });

  it("reports ready for Microsoft, which carries no secret", () => {
    // GIVEN a public client: an application id and no secret, by design
    const status = statusOf({
      provider: "microsoft",
      requires_client_secret: false,
      has_client_secret: false,
      client_secret_source: "none",
    });

    // WHEN readiness is derived
    // THEN the absent secret is not held against it
    expect(oauthReadiness(status)).toBe("ready");
    expect(needsOauthSetup(status)).toBe(false);
  });

  it("reports ready for Microsoft on a fresh install, from the shipped client", () => {
    // GIVEN a fresh install that has configured nothing: Microsoft resolves
    // from the client id Apollia ships, so its source is "builtin"
    const status = statusOf({
      provider: "microsoft",
      source: "builtin",
      effective_client_id: "c4f95bc5-8895-4550-8119-ed0e548fd941",
      requires_client_secret: false,
      has_client_secret: false,
      client_secret_source: "none",
      requires_api_key: false,
    });

    // WHEN readiness is derived
    // THEN Microsoft is connectable with nothing to configure, so the detail
    // pane must offer "Connect" and not the setup prompt Google still gets
    expect(oauthReadiness(status)).toBe("ready");
    expect(needsOauthSetup(status)).toBe(false);
  });

  it("ignores the Picker API key, which gates Drive rather than connecting", () => {
    // GIVEN complete OAuth credentials but no API key
    const status = statusOf({ requires_api_key: true, has_api_key: false });

    // WHEN readiness is derived
    // THEN the connector is still connectable
    expect(oauthReadiness(status)).toBe("ready");
  });

  it("treats an unknown provider as needing setup", () => {
    // GIVEN no row at all for a provider, which is what a failed status load
    // leaves behind
    // WHEN the caller asks whether it needs setup
    // THEN it errs toward telling the operator, not toward a dead Connect button
    expect(needsOauthSetup(undefined)).toBe(true);
  });
});
