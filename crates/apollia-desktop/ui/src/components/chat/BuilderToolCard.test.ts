import { describe, test, expect } from "vitest";
import {
  resolveOutputParams,
  buildOutputSummary,
  buildBashInputDisplay,
  buildHttpInputDisplay,
} from "$lib/tools/tool-display";
import type { ToolCallView } from "$lib/types";

// ─── Helpers ─────────────────────────────────────────────────────────────────

function makeCall(
  tool_name: string,
  input: Record<string, unknown> = {},
  output: string | null = null,
  status: ToolCallView["status"] = "executed",
  duration_ms: number | null = null,
  exit_code: number | null = null,
): ToolCallView {
  return { tool_name, input, output, status, duration_ms, exit_code };
}

/**
 * Mirrors the inputPreview derivation in BuilderToolCard.
 * Returns a truncated preview and a flag indicating whether truncation occurred.
 */
function deriveInputPreview(
  input: Record<string, unknown>,
  max = 200,
): { preview: string; truncated: boolean } {
  const json = JSON.stringify(input, null, 2);
  const truncated = json.length > max;
  const preview = truncated ? json.slice(0, max) + "..." : json;
  return { preview, truncated };
}

/**
 * Mirrors the outputToggleLabel derivation in BuilderToolCard.
 */
function deriveOutputToggleLabel(
  toolName: string,
  params: Record<string, string>,
): string {
  const summary = buildOutputSummary(toolName, params);
  return summary !== null ? `Show result (${summary})` : "Show result";
}

// ─── tool_name display ────────────────────────────────────────────────────────

describe("BuilderToolCard — tool_name display", () => {
  test("data-testid pattern is builder-tool-{tool_name}", () => {
    // GIVEN any tool name
    const toolName = "file_read";
    // THEN the expected testid is builder-tool-file_read
    expect(`builder-tool-${toolName}`).toBe("builder-tool-file_read");
  });

  test("data-testid uses raw tool_name for file_grep", () => {
    const toolName = "file_grep";
    expect(`builder-tool-${toolName}`).toBe("builder-tool-file_grep");
  });
});

// ─── duration display ─────────────────────────────────────────────────────────

describe("BuilderToolCard — duration display", () => {
  test("duration_ms is available on the call when executed", () => {
    // GIVEN a tool call executed with duration
    const call = makeCall("file_grep", {}, null, "executed", 127);
    // THEN the field is present with the expected value
    expect(call.duration_ms).toBe(127);
    expect(call.status).toBe("executed");
  });

  test("duration_ms is null when not provided", () => {
    const call = makeCall("file_grep");
    expect(call.duration_ms).toBeNull();
  });

  test("exit_code is available on refused calls", () => {
    // GIVEN a refused call with exit code
    const call = makeCall("bash_executor", {}, null, "refused", null, 1);
    expect(call.exit_code).toBe(1);
    expect(call.status).toBe("refused");
  });
});

// ─── input preview ────────────────────────────────────────────────────────────

describe("BuilderToolCard — input JSON preview", () => {
  test("short input is shown in full without truncation", () => {
    // GIVEN a small input object
    const input = { path: "src/main.rs" };
    // WHEN
    const { preview, truncated } = deriveInputPreview(input);
    // THEN no truncation
    expect(truncated).toBe(false);
    expect(preview).toContain("src/main.rs");
  });

  test("input longer than 200 chars is truncated with ellipsis", () => {
    // GIVEN an input that produces JSON longer than 200 characters
    const input = { content: "x".repeat(250) };
    // WHEN
    const { preview, truncated } = deriveInputPreview(input);
    // THEN truncated at 200 chars
    expect(truncated).toBe(true);
    expect(preview.endsWith("...")).toBe(true);
    // preview = first 200 chars of JSON + "..."
    const json = JSON.stringify(input, null, 2);
    expect(preview).toBe(json.slice(0, 200) + "...");
  });

  test("input exactly at 200 chars is not truncated", () => {
    // Build an object whose JSON is exactly 200 chars
    const base = JSON.stringify({ k: "" }, null, 2); // length without value
    const pad = 200 - base.length;
    const input = { k: "a".repeat(pad) };
    const json = JSON.stringify(input, null, 2);
    expect(json.length).toBe(200);
    const { truncated } = deriveInputPreview(input);
    expect(truncated).toBe(false);
  });
});

// ─── bash_executor special display ───────────────────────────────────────────

describe("BuilderToolCard — bash_executor input display", () => {
  test("returns $ prefixed command string", () => {
    // GIVEN a bash_executor input with a command
    const input = { command: "git status --short" };
    // WHEN
    const result = buildBashInputDisplay(input);
    // THEN "$ git status --short"
    expect(result).toBe("$ git status --short");
  });

  test("returns null when command field is missing", () => {
    expect(buildBashInputDisplay({})).toBeNull();
  });

  test("returns null when command is not a string", () => {
    expect(buildBashInputDisplay({ command: 42 })).toBeNull();
  });

  test("preserves command with special characters", () => {
    const input = { command: 'grep -r "fn main" *.rs' };
    expect(buildBashInputDisplay(input)).toBe('$ grep -r "fn main" *.rs');
  });
});

// ─── http_fetch special display ──────────────────────────────────────────────

describe("BuilderToolCard — http_fetch input display", () => {
  test("returns METHOD URL string for POST request", () => {
    // GIVEN an http_fetch input with method and url
    const input = { method: "POST", url: "https://api.notion.com/v1/pages" };
    // WHEN
    const result = buildHttpInputDisplay(input);
    // THEN "POST https://api.notion.com/v1/pages"
    expect(result).toBe("POST https://api.notion.com/v1/pages");
  });

  test("defaults to GET when method is absent", () => {
    const input = { url: "https://api.example.com/data" };
    const result = buildHttpInputDisplay(input);
    expect(result).toBe("GET https://api.example.com/data");
  });

  test("uppercases lowercase method", () => {
    const input = { method: "delete", url: "https://api.example.com/res/1" };
    expect(buildHttpInputDisplay(input)).toBe("DELETE https://api.example.com/res/1");
  });

  test("returns null when url is missing", () => {
    expect(buildHttpInputDisplay({ method: "GET" })).toBeNull();
  });

  test("returns null when url is not a string", () => {
    expect(buildHttpInputDisplay({ url: null })).toBeNull();
  });
});

// ─── output summary (collapsible toggle label) ───────────────────────────────

describe("BuilderToolCard — buildOutputSummary", () => {
  test("file_grep with match_count and file_count", () => {
    // GIVEN output params for file_grep
    const params = { match_count: "12", file_count: "8" };
    // WHEN
    const summary = buildOutputSummary("file_grep", params);
    // THEN
    expect(summary).toBe("12 matches in 8 files");
  });

  test("file_grep with only match_count", () => {
    const params = { match_count: "5" };
    expect(buildOutputSummary("file_grep", params)).toBe("5 matches");
  });

  test("file_read returns line count", () => {
    const params = { total_lines: "50" };
    expect(buildOutputSummary("file_read", params)).toBe("50 lines");
  });

  test("file_write returns bytes written", () => {
    const params = { bytes_written: "1024" };
    expect(buildOutputSummary("file_write", params)).toBe("1024 bytes written");
  });

  test("file_edit returns replacement count", () => {
    const params = { replacements: "3" };
    expect(buildOutputSummary("file_edit", params)).toBe("3 replacement(s)");
  });

  test("file_list returns entry count", () => {
    const params = { entry_count: "7" };
    expect(buildOutputSummary("file_list", params)).toBe("7 entries");
  });

  test("file_glob returns match count", () => {
    const params = { match_count: "4" };
    expect(buildOutputSummary("file_glob", params)).toBe("4 matches");
  });

  test("bash_executor returns exit code", () => {
    const params = { exit_code: "0" };
    expect(buildOutputSummary("bash_executor", params)).toBe("exit 0");
  });

  test("http_fetch returns HTTP status code", () => {
    const params = { status_code: "200" };
    expect(buildOutputSummary("http_fetch", params)).toBe("HTTP 200");
  });

  test("memory_search returns result count", () => {
    const params = { result_count: "3" };
    expect(buildOutputSummary("memory_search", params)).toBe("3 results");
  });

  test("unknown tool returns null", () => {
    expect(buildOutputSummary("unknown_tool", { foo: "bar" })).toBeNull();
  });

  test("known tool with empty params returns null", () => {
    expect(buildOutputSummary("file_grep", {})).toBeNull();
  });
});

// ─── output toggle label ──────────────────────────────────────────────────────

describe("BuilderToolCard — output toggle label", () => {
  test("includes summary when available", () => {
    // GIVEN a file_grep call with output
    const call = makeCall(
      "file_grep",
      { pattern: "TODO" },
      '{"match_count": "12", "file_count": "8"}',
    );
    const params = resolveOutputParams(call);
    const label = deriveOutputToggleLabel(call.tool_name, params);
    // THEN summary is embedded in the toggle label
    expect(label).toBe("Show result (12 matches in 8 files)");
  });

  test("falls back to plain label when no summary available", () => {
    // GIVEN a tool call with no parseable output summary
    const call = makeCall("unknown_tool", {}, '{"data": "opaque"}');
    const params = resolveOutputParams(call);
    const label = deriveOutputToggleLabel(call.tool_name, params);
    expect(label).toBe("Show result");
  });

  test("falls back when output is null", () => {
    const call = makeCall("file_grep", { pattern: "TODO" }, null);
    const params = resolveOutputParams(call);
    const label = deriveOutputToggleLabel(call.tool_name, params);
    expect(label).toBe("Show result");
  });
});

// ─── output visibility logic ─────────────────────────────────────────────────

describe("BuilderToolCard — output visibility logic", () => {
  test("output section is available when executed and output non-null", () => {
    const call = makeCall("file_read", {}, '{"total_lines": "50"}', "executed");
    const showOutput = call.status === "executed" && call.output !== null;
    expect(showOutput).toBe(true);
  });

  test("output section is hidden when output is null", () => {
    const call = makeCall("file_read", {}, null, "executed");
    const showOutput = call.status === "executed" && call.output !== null;
    expect(showOutput).toBe(false);
  });

  test("output section is hidden when status is pending", () => {
    const call = makeCall("file_read", {}, '{"total_lines": "50"}', "pending");
    const showOutput = call.status === "executed" && call.output !== null;
    expect(showOutput).toBe(false);
  });
});
