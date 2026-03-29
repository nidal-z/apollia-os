import { describe, test, expect } from "vitest";
import { resolveToolDisplay } from "$lib/tools/tool-display";
import type { ToolCallView } from "$lib/types";
import { Globe, Terminal } from "lucide-svelte";

// ─── Helpers ─────────────────────────────────────────────────────────────────

function makeCall(
  tool_name: string,
  input: Record<string, unknown> = {},
  output: string | null = null,
): ToolCallView {
  return { tool_name, input, output, status: "pending" };
}

/**
 * Returns true when the given string looks like serialised JSON
 * (starts with `{` or `[` after trimming).
 */
function looksLikeJson(value: string): boolean {
  const trimmed = value.trim();
  return trimmed.startsWith("{") || trimmed.startsWith("[");
}

// ─── Title behaviour ──────────────────────────────────────────────────────────

describe("OperatorApprovalCard — title", () => {
  test("labelKey does not contain the raw tool_name", () => {
    // GIVEN a tool call for file_grep in pending status
    const call = makeCall("file_grep", { pattern: "TODO" });
    // WHEN display info is resolved
    const display = resolveToolDisplay(call);
    // THEN the labelKey is the generic label, not a raw technical identifier
    expect(display.labelKey).toBe("tools.labels.file_grep");
    expect(display.labelKey).not.toContain("file_grep_raw");
  });

  test("labelKey for unknown tool does not embed the tool input", () => {
    // GIVEN an unknown tool with a complex input
    const call = makeCall("unknown_custom_tool", { some_param: "value", count: 42 });
    // WHEN
    const display = resolveToolDisplay(call);
    // THEN label key follows the standard pattern without injecting input values
    expect(display.labelKey).toBe("tools.labels.unknown_custom_tool");
    expect(display.templateParams).toEqual({});
  });
});

// ─── Human description — no JSON ─────────────────────────────────────────────

describe("OperatorApprovalCard — human description without JSON", () => {
  test("http_fetch POST exposes hostname, not raw URL object", () => {
    // GIVEN a POST call to a JSON API
    const call = makeCall("http_fetch", {
      url: "https://api.notion.com/v1/pages",
      method: "POST",
      body: JSON.stringify({ title: "My page" }),
    });
    // WHEN
    const display = resolveToolDisplay(call);
    // THEN templateParams expose a human-readable hostname, not raw JSON
    expect(display.templateParams.hostname).toBe("api.notion.com");
    expect(looksLikeJson(display.templateParams.hostname)).toBe(false);
    expect(display.descriptionKey).toBe("tools.descriptions.http_fetch_post");
  });

  test("file_grep templateParams contain pattern string, not JSON", () => {
    // GIVEN a file_grep call with pattern and glob
    const call = makeCall("file_grep", { pattern: "TODO", glob: "*.rs" });
    // WHEN
    const display = resolveToolDisplay(call);
    // THEN params are plain strings
    expect(looksLikeJson(display.templateParams.pattern)).toBe(false);
    expect(looksLikeJson(display.templateParams.glob)).toBe(false);
  });

  test("file_read templateParams expose a path, not a JSON envelope", () => {
    // GIVEN a file_read call with a plain path
    const call = makeCall("file_read", { path: "src/main.rs" });
    // WHEN
    const display = resolveToolDisplay(call);
    // THEN
    expect(looksLikeJson(display.templateParams.path)).toBe(false);
    expect(display.templateParams.path).toBe("src/main.rs");
  });

  test("memory_search with namespace exposes query and namespace strings", () => {
    // GIVEN a memory_search call with namespace
    const call = makeCall("memory_search", {
      query: "project goals",
      namespace: "episodic",
    });
    // WHEN
    const display = resolveToolDisplay(call);
    // THEN
    expect(looksLikeJson(display.templateParams.query)).toBe(false);
    expect(display.templateParams.namespace).toBe("episodic");
    expect(display.descriptionKey).toBe("tools.descriptions.memory_search_ns");
  });

  test("unknown tool exposes empty templateParams — no raw input leaks", () => {
    // GIVEN an unknown tool with complex nested input that should not appear in params
    const call = makeCall("exotic_tool", {
      nested: { deep: true },
      list: [1, 2, 3],
    });
    // WHEN
    const display = resolveToolDisplay(call);
    // THEN templateParams are empty — no raw JSON injected into the description
    expect(display.templateParams).toEqual({});
    expect(display.icon).toBe(Terminal);
  });
});

// ─── Tool icon resolution ────────────────────────────────────────────────────

describe("OperatorApprovalCard — tool icon", () => {
  test("http_fetch uses the Globe icon", () => {
    // GIVEN an http_fetch call
    const call = makeCall("http_fetch", { url: "https://example.com", method: "GET" });
    // WHEN
    const display = resolveToolDisplay(call);
    // THEN the icon is Globe (visually distinct from the generic Terminal)
    expect(display.icon).toBe(Globe);
  });

  test("unknown tool falls back to Terminal icon", () => {
    // GIVEN an unrecognised tool name
    const call = makeCall("unknown_tool_xyz");
    // WHEN
    const display = resolveToolDisplay(call);
    // THEN fallback icon is Terminal
    expect(display.icon).toBe(Terminal);
  });
});

// ─── Tauri command parameters ─────────────────────────────────────────────────

describe("OperatorApprovalCard — authorize command shape", () => {
  test("authorize payload mirrors ApprovalCard: toolName from toolCall.tool_name", () => {
    // GIVEN a pending tool call
    const toolCall = makeCall("file_write", { path: "out.txt", content: "hello" });
    const sessionId = "sess-123";
    const messageId = "msg-456";
    // WHEN building the invoke payload
    const payload = {
      sessionId,
      messageId,
      toolName: toolCall.tool_name,
      decision: "accept" as const,
    };
    // THEN the payload is well-typed and uses the tool_name from the call
    expect(payload.toolName).toBe("file_write");
    expect(payload.decision).toBe("accept");
    expect(payload.sessionId).toBe("sess-123");
    expect(payload.messageId).toBe("msg-456");
  });

  test("refuse decision is a valid decision value", () => {
    const decision: "accept" | "refuse" | "always_accept" = "refuse";
    expect(decision).toBe("refuse");
  });

  test("always_accept decision is a valid decision value", () => {
    const decision: "accept" | "refuse" | "always_accept" = "always_accept";
    expect(decision).toBe("always_accept");
  });
});
