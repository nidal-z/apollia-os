import { describe, it, expect } from "vitest";
import { toolKindTag } from "$lib/ipc/tools";

describe("toolKindTag", () => {
  it("reads the discriminant of a unit variant", () => {
    // GIVEN the wire form of ToolKind::Native, an internally tagged object
    const kind = { type: "native" };

    // WHEN the discriminant is extracted
    const tag = toolKindTag(kind);

    // THEN the snake_case tag is returned
    expect(tag).toBe("native");
  });

  it("reads the discriminant of a variant carrying a payload", () => {
    // GIVEN the wire form of ToolKind::McpServer with its payload fields
    const kind = {
      type: "mcp_server",
      server_url: "http://localhost:3000",
      transport: "http",
      tool_name: "search",
    };

    // WHEN the discriminant is extracted
    const tag = toolKindTag(kind);

    // THEN the payload is ignored and only the tag comes back
    expect(tag).toBe("mcp_server");
  });

  it("accepts a pre-flattened string tag", () => {
    // GIVEN a backend that already flattened the enum to its tag
    const kind = "custom";

    // WHEN the discriminant is extracted
    const tag = toolKindTag(kind);

    // THEN the string passes through unchanged
    expect(tag).toBe("custom");
  });

  it("returns null when no usable tag is present", () => {
    // GIVEN payloads that carry no discriminant
    const candidates: unknown[] = [null, undefined, "", {}, { type: 42 }, []];

    // WHEN each is read
    const tags = candidates.map(toolKindTag);

    // THEN none yields a tag, so the badge stays hidden
    expect(tags).toEqual([null, null, null, null, null, null]);
  });
});
