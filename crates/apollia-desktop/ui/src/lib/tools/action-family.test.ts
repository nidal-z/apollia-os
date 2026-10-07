import { describe, expect, test } from "vitest";
import { isSensitiveFamily } from "$lib/tools/action-family";

describe("isSensitiveFamily", () => {
  test("only reading and the exempt family are not sensitive", () => {
    // GIVEN every family, plus a value no build knows
    // WHEN each is classified
    // THEN reading and exempt are the only non-sensitive values
    expect(isSensitiveFamily("read")).toBe(false);
    expect(isSensitiveFamily("exempt")).toBe(false);
    for (const family of [
      "read_personal",
      "write_or_delete",
      "send_external",
      "execute_code",
      "unclassified",
      "teleport",
    ]) {
      expect(isSensitiveFamily(family)).toBe(true);
    }
  });
});
