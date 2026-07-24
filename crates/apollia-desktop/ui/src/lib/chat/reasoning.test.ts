/**
 * Tests for the ReasoningItem normalizer.
 *
 * Covers the discriminated kinds, the uniform tool-call row that web tools fold
 * into, and the `web_search` / `web_read` rich-output parsers that restore the
 * clickable results body in `ReasoningCard.svelte`.
 */

import { describe, expect, test } from "vitest";
import type { ToolCallView } from "../types";
import {
  buildReasoningSequence,
  parseWebReadOutput,
  parseWebSearchOutput,
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

describe("toReasoningItem - web tools fold into the flat tool row", () => {
  test("web_search renders as a tool_call, not a specialized kind", () => {
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
    expect(item.kind).toBe("tool_call");
    if (item.kind === "tool_call") {
      expect(item.tool).toBe("web_search");
      // The rich result JSON stays visible in the tool output, like any tool.
      expect(item.output).toContain("tokio.rs");
      // ...and parses back into a clickable results body.
      const parsed = parseWebSearchOutput(item);
      expect(parsed?.results).toHaveLength(1);
      expect(parsed?.results[0]?.url).toBe("https://tokio.rs");
      expect(parsed?.query).toBe("rust async");
    }
  });

  test("web_read renders as a tool_call, keeping its output", () => {
    const call = makeCall({
      tool_name: "web_read",
      input: { url: "https://example.com" },
      output: JSON.stringify({
        url: "https://example.com",
        title: "Example",
        content: "Hello world",
      }),
    });
    const item = toReasoningItem(call, "msg", 1);
    expect(item.kind).toBe("tool_call");
    if (item.kind === "tool_call") {
      expect(item.tool).toBe("web_read");
      expect(item.output).toContain("Hello world");
      const parsed = parseWebReadOutput(item);
      expect(parsed?.title).toBe("Example");
      expect(parsed?.extracted).toBe("Hello world");
      expect(parsed?.url).toBe("https://example.com");
    }
  });

  test("parseWebSearchOutput returns null on non-JSON output", () => {
    expect(
      parseWebSearchOutput({ input: {}, output: "not json", duration_ms: null }),
    ).toBeNull();
    expect(
      parseWebReadOutput({ input: {}, output: null, duration_ms: null }),
    ).toBeNull();
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

  test("interleaves fragments with tool calls per reasoning_boundaries", () => {
    // Two steps: think -> tool0, think -> tool1. Boundaries [0, 1] mean
    // fragment 0 precedes tool 0 and fragment 1 precedes tool 1.
    const items = buildReasoningSequence({
      id: "m3",
      tool_calls: [
        makeCall({ tool_name: "file_read" }),
        makeCall({ tool_name: "file_write" }),
      ],
      metadata: {
        thinking_trace: "Plan read.\n\n---\n\nNow write.",
        reasoning_boundaries: [0, 1],
      },
    });
    expect(
      items.map((i) => (i.kind === "thinking" ? "T" : "C")).join(""),
    ).toBe("TCTC");
  });

  test("falls back to fragments-then-tools when boundaries mismatch", () => {
    const items = buildReasoningSequence({
      id: "m4",
      tool_calls: [makeCall({ tool_name: "file_read" })],
      metadata: {
        thinking_trace: "a\n\n---\n\nb",
        reasoning_boundaries: [0], // length 1 != 2 fragments
      },
    });
    expect(
      items.map((i) => (i.kind === "thinking" ? "T" : "C")).join(""),
    ).toBe("TTC");
  });

  test("splits a multi-fragment thinking_trace into separate captions", () => {
    const items = buildReasoningSequence({
      id: "m2",
      tool_calls: [makeCall({ tool_name: "file_read" })],
      metadata: { thinking_trace: "First I plan.\n\n---\n\nThen I act." },
    });
    const thinking = items.filter((i) => i.kind === "thinking");
    expect(thinking).toHaveLength(2);
    expect(thinking[0].kind === "thinking" && thinking[0].content).toBe(
      "First I plan.",
    );
    expect(thinking[1].kind === "thinking" && thinking[1].content).toBe(
      "Then I act.",
    );
    expect(items[items.length - 1]?.kind).toBe("tool_call");
  });
});

describe("ReasoningItem - all kinds are constructible", () => {
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
      "thinking",
      "rationale",
      "retry",
      "citation",
    ]);
  });
});
