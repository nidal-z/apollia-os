import { describe, test, expect } from "vitest";
import { HITL_CONFIRMED_NATIVE_TOOLS, isSensitiveTool } from "./sensitiveTools";

/**
 * Unit tests for the sensitivity predicate the agent Tools tab renders as
 * the "asks for confirmation on each call" badge.
 *
 * The Svelte component itself is exercised through the dev app; these tests
 * cover the predicate it delegates to, following the QuickPicker precedent.
 */

describe("isSensitiveTool", () => {
  test("lights the badge on every native tool the dispatcher confirms", () => {
    // GIVEN the six natives the runtime wraps behind a human confirmation
    const confirmed = [
      "bash_executor",
      "python_executor",
      "file_write",
      "file_edit",
      "notebook_edit",
      "http_fetch",
    ];

    // WHEN the badge predicate evaluates each of them
    const lit = confirmed.filter((id) => isSensitiveTool(id));

    // THEN all six light the badge
    expect(lit).toEqual(confirmed);
  });

  test("stays off on natives the dispatcher does not confirm", () => {
    // GIVEN natives the dispatcher builds untouched
    const untouched = [
      "file_read",
      "file_list",
      "file_glob",
      "file_grep",
      "notebook_read",
      "web_search",
      "web_read",
      "memory_search",
      "ask_user",
    ];

    // WHEN the badge predicate evaluates each of them
    const lit = untouched.filter((id) => isSensitiveTool(id));

    // THEN none lights the badge
    expect(lit).toEqual([]);
  });

  test("keeps the send and delete heuristics for non-native tool ids", () => {
    // GIVEN external tool ids carrying a send or delete verb
    const external = ["gmail_send_email", "calendar.delete_event"];

    // WHEN the badge predicate evaluates each of them
    const lit = external.filter((id) => isSensitiveTool(id));

    // THEN both light the badge
    expect(lit).toEqual(external);
  });

  test("the exported native list is exactly the dispatcher set", () => {
    // GIVEN the list this module exports
    // WHEN compared with the dispatcher's WRAPPED_NATIVES set
    // THEN they hold the same six names
    expect([...HITL_CONFIRMED_NATIVE_TOOLS].sort()).toEqual(
      [
        "bash_executor",
        "python_executor",
        "file_write",
        "file_edit",
        "notebook_edit",
        "http_fetch",
      ].sort(),
    );
  });
});
