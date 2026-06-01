/**
 * Tests for the ReasoningItem normalizer.
 *
 * Covers the 7 discriminated kinds + the tool-call → web_search/web_read
 * promotion path used by `ReasoningCard.svelte`.
 */

import { describe, expect, test } from "vitest";
import type { ToolCallView } from "../types";
import {
  buildReasoningSequence,
  toReasoningItem,
  type ReasoningItem,
} from "./reasoning";

function makeCall(
  overrides: Partial<ToolCallView> & { tool_name: string },
): ToolCallView {
  return {
    tool_name: overrides.tool_name,
    input: overrides.input ?? {},
    output: overrides.output ?? null,
    status: overrides.status ?? "executed",
    duration_ms: overrides.duration_ms ?? null,
    exit_code: overrides.exit_code ?? null,
  };
}

describe("toReasoningItem - generic tool_call kind", () => {
  test("GIVEN a plain tool call WHEN normalized THEN kind is tool_call", () => {
    const call = makeCall({
      tool_name: "file_read",
      input: { path: "/etc/hosts" },
      output: "127.0.0.1",
      duration_ms: 12,
    });
    const item = toReasoningItem(call, "msg", 0);
    expect(item.kind).toBe("tool_call");
    expect(item.id).toBe("msg-0");
    expect(item.status).toBe("success");
    if (item.kind === "tool_call") {
      expect(item.tool).toBe("file_read");
      expect(item.args).toEqual({ path: "/etc/hosts" });
    }
  });

  test("maps ToolCallView.status → ReasoningStatus", () => {
    expect(toReasoningItem(makeCall({ tool_name: "x", status: "pending" }), "m", 0).status).toBe(
      "pending",
    );
    expect(toReasoningItem(makeCall({ tool_name: "x", status: "authorized" }), "m", 0).status).toBe(
      "running",
    );
    expect(toReasoningItem(makeCall({ tool_name: "x", status: "executed" }), "m", 0).status).toBe(
      "success",
    );
    expect(toReasoningItem(makeCall({ tool_name: "x", status: "refused" }), "m", 0).status).toBe(
      "rejected",
    );
  });
});

describe("toReasoningItem - web_search promotion", () => {
  test("promotes to web_search kind when output parses", () => {
    const call = makeCall({
      tool_name: "web_search",
      input: { query: "rust async" },
      output: JSON.stringify({
        query: "rust async",
        backend: "ddg",
        results: [
          { title: "Tokio", url: "https://tokio.rs", snippet: "", rank: 1 },
        ],
        total_results: 1,
        duration_ms: 42,
      }),
    });
    const item = toReasoningItem(call, "msg", 3);
    expect(item.kind).toBe("web_search");
    if (item.kind === "web_search") {
      expect(item.query).toBe("rust async");
      expect(item.results).toHaveLength(1);
      expect(item.backend).toBe("ddg");
    }
  });

  test("falls back to tool_call kind when output is not parseable", () => {
    const call = makeCall({
      tool_name: "web_search",
      input: { query: "x" },
      output: "not-json",
    });
    const item = toReasoningItem(call, "msg", 0);
    expect(item.kind).toBe("tool_call");
  });
});

describe("toReasoningItem - web_read promotion", () => {
  test("promotes to web_read kind with extracted content", () => {
    const call = makeCall({
      tool_name: "web_read",
      input: { url: "https://example.com" },
      output: JSON.stringify({
        url: "https://example.com",
        title: "Example",
        byline: null,
        content: "Hello world",
        chars_total: 11,
        truncated: false,
        duration_ms: 88,
      }),
    });
    const item = toReasoningItem(call, "msg", 1);
    expect(item.kind).toBe("web_read");
    if (item.kind === "web_read") {
      expect(item.url).toBe("https://example.com");
      expect(item.extracted).toBe("Hello world");
      expect(item.title).toBe("Example");
    }
  });
});

describe("buildReasoningSequence", () => {
  test("prepends a thinking item when metadata.thinking_trace is set", () => {
    const items = buildReasoningSequence({
      id: "m1",
      tool_calls: [makeCall({ tool_name: "file_read" })],
      metadata: { thinking_trace: "I should read the file." },
    });
    expect(items[0]?.kind).toBe("thinking");
    if (items[0]?.kind === "thinking") {
      expect(items[0].content).toBe("I should read the file.");
    }
    expect(items[1]?.kind).toBe("tool_call");
  });

  test("returns empty sequence when nothing to show", () => {
    const items = buildReasoningSequence({
      id: "m0",
      tool_calls: null,
      metadata: null,
    });
    expect(items).toEqual([]);
  });
});

describe("ReasoningItem - all 7 kinds are constructible", () => {
  test("snapshot: one instance of each kind", () => {
    const items: ReasoningItem[] = [
      {
        id: "a",
        kind: "tool_call",
        status: "success",
        tool: "file_read",
        args: {},
        output: null,
      },
      {
        id: "b",
        kind: "web_search",
        status: "success",
        query: "q",
        results: [],
      },
      {
        id: "c",
        kind: "web_read",
        status: "success",
        url: "https://ex.com",
        extracted: "text",
      },
      {
        id: "d",
        kind: "thinking",
        status: "success",
        content: "…",
      },
      {
        id: "e",
        kind: "rationale",
        status: "success",
        content: "because",
      },
      {
        id: "f",
        kind: "retry",
        status: "error",
        attempts: [
          { index: 1, status: "error", error: "boom" },
          { index: 2, status: "success" },
        ],
      },
      {
        id: "g",
        kind: "citation",
        status: "success",
        source: "doc.pdf",
        excerpt: "quote",
      },
    ];
    expect(items.map((i) => i.kind)).toEqual([
      "tool_call",
      "web_search",
      "web_read",
      "thinking",
      "rationale",
      "retry",
      "citation",
    ]);
  });
});
