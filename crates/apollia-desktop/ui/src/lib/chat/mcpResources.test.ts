import { describe, it, expect, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import {
  detectMentionQuery,
  filterResources,
  buildPinnedPrefix,
  type McpResourceView,
  type PinnedResource,
} from "./mcpResources";

const RESOURCES: McpResourceView[] = [
  { server: "notion", uri: "notion://page/1", name: "Roadmap", description: "Q3 plan" },
  { server: "files", uri: "file:///docs/spec.md", name: "Spec", mime_type: "text/markdown" },
];

describe("detectMentionQuery", () => {
  it("returns null when there is no @ before the cursor", () => {
    expect(detectMentionQuery("hello world", 11)).toBeNull();
    expect(detectMentionQuery("", 0)).toBeNull();
  });

  it("returns the token after a leading @", () => {
    expect(detectMentionQuery("@road", 5)).toBe("road");
    expect(detectMentionQuery("@", 1)).toBe("");
  });

  it("triggers when the @ follows whitespace", () => {
    expect(detectMentionQuery("see @spec", 9)).toBe("spec");
  });

  it("does not trigger on inline foo@bar (no whitespace before @)", () => {
    expect(detectMentionQuery("mail me at foo@bar", 18)).toBeNull();
  });

  it("closes once whitespace follows the token", () => {
    expect(detectMentionQuery("@road map", 9)).toBeNull();
  });
});

describe("filterResources", () => {
  it("returns all resources for an empty query", () => {
    expect(filterResources(RESOURCES, "")).toHaveLength(2);
  });

  it("filters by name, uri, or server (case-insensitive)", () => {
    expect(filterResources(RESOURCES, "ROAD").map((r) => r.uri)).toEqual([
      "notion://page/1",
    ]);
    expect(filterResources(RESOURCES, "spec.md").map((r) => r.server)).toEqual([
      "files",
    ]);
    expect(filterResources(RESOURCES, "notion")).toHaveLength(1);
  });
});

describe("buildPinnedPrefix", () => {
  it("returns an empty string when nothing is pinned", () => {
    expect(buildPinnedPrefix([])).toBe("");
  });

  it("wraps pinned resources in a system-prefix block referencing the read tool", () => {
    const pinned: PinnedResource[] = [
      { server: "notion", uri: "notion://page/1", name: "Roadmap" },
    ];
    const prefix = buildPinnedPrefix(pinned);
    expect(prefix).toContain("<pinned-mcp-resources>");
    expect(prefix).toContain("</pinned-mcp-resources>");
    expect(prefix).toContain("mcp_resources_read");
    expect(prefix).toContain('uri="notion://page/1"');
  });
});
