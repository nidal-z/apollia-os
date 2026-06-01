import { describe, test, expect } from "vitest";
import { classifySessionError } from "./runtimeHealth";

/**
 * Unit tests for the session-error classifier.
 *
 * The classifier powers the SessionNotFound vs session-corrupted routing
 * in `ChatConversation.svelte` - a regression here silently downgrades
 * the banner to the generic "other" fallback.
 */

describe("classifySessionError", () => {
  test("maps missing-session messages to not_found", () => {
    expect(classifySessionError("Session not found: abc123")).toBe("not_found");
    expect(classifySessionError("No row returned for session")).toBe("not_found");
    expect(classifySessionError("unknown session id")).toBe("not_found");
  });

  test("maps backend persistence failures to corrupted", () => {
    expect(classifySessionError("sqlite error: malformed database")).toBe(
      "corrupted",
    );
    expect(classifySessionError("Database file is corrupt")).toBe("corrupted");
    expect(
      classifySessionError("serde_json: failed to decode messages column"),
    ).toBe("corrupted");
  });

  test("falls back to other for unrelated errors", () => {
    expect(classifySessionError("permission denied")).toBe("other");
    expect(classifySessionError("timeout")).toBe("other");
    expect(classifySessionError("")).toBe("other");
  });

  test("is case-insensitive", () => {
    expect(classifySessionError("SESSION NOT FOUND")).toBe("not_found");
    expect(classifySessionError("SQLite panic")).toBe("corrupted");
  });
});
