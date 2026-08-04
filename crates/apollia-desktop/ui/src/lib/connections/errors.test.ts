import { describe, it, expect } from "vitest";
import { isMissingClientError, isMissingSecretError, formatTauriError } from "./errors";

/** Identity translator: asserts on the key rather than on localized copy. */
const echo = (key: string): string => key;

describe("isMissingSecretError", () => {
  it("recognises the missing-secret kind", () => {
    // GIVEN the error the connect guard raises when Google's secret is absent
    const raw = { kind: "oauth_client_secret_missing", detail: "google" };

    // WHEN the dialog classifies it
    const isSecret = isMissingSecretError(raw);

    // THEN it can show the banner that names the secret
    expect(isSecret).toBe(true);
  });

  it("does not confuse a missing secret with a missing client id", () => {
    // GIVEN the older, different refusal
    const raw = { kind: "oauth_client_not_configured", detail: "google" };

    // WHEN both classifiers run
    // THEN each claims only its own kind, so the two banners stay distinct
    expect(isMissingClientError(raw)).toBe(true);
    expect(isMissingSecretError(raw)).toBe(false);
  });

  it("tolerates a null or unshaped value", () => {
    // GIVEN values that are not Tauri errors at all
    // WHEN classified
    // THEN nothing throws and nothing matches
    expect(isMissingSecretError(null)).toBe(false);
    expect(isMissingSecretError("boom")).toBe(false);
    expect(isMissingSecretError({})).toBe(false);
  });
});

describe("formatTauriError", () => {
  it("maps the missing-secret kind to its own copy", () => {
    // GIVEN the missing-secret refusal
    const raw = { kind: "oauth_client_secret_missing", detail: "google" };

    // WHEN it is formatted for display
    const text = formatTauriError(raw, echo);

    // THEN the operator gets the dedicated message, not the raw kind
    expect(text).toBe("connections.error_oauth_client_secret_missing");
  });

  it("appends the parser detail to the invalid-file message", () => {
    // GIVEN a credentials file the parser rejected, with its reason
    const raw = { kind: "invalid_client_file", detail: "no client_id field" };

    // WHEN it is formatted
    const text = formatTauriError(raw, echo);

    // THEN the actionable half survives alongside the localized label
    expect(text).toBe("connections.error_invalid_client_file no client_id field");
  });

  it("keeps the label alone when the file error carries no detail", () => {
    // GIVEN the same kind without a reason
    const raw = { kind: "invalid_client_file" };

    // WHEN it is formatted
    const text = formatTauriError(raw, echo);

    // THEN there is no dangling separator
    expect(text).toBe("connections.error_invalid_client_file");
  });

  it("falls back to kind and detail for an unknown kind", () => {
    // GIVEN a kind this module does not localize
    const raw = { kind: "some_new_kind", detail: "context" };

    // WHEN it is formatted
    const text = formatTauriError(raw, echo);

    // THEN nothing is swallowed
    expect(text).toBe("some_new_kind: context");
  });
});
