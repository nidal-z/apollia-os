import { describe, test, expect } from "vitest";
import {
  resolveToolDisplay,
  resolveOutputParams,
  truncatePath,
  truncateString,
  formatRationale,
  humanizeToolError,
  summariseJsonOutput,
} from "./tool-display";
import type { ToolCallView } from "$lib/types";
import {
  FileText,
  FilePen,
  FilePenLine,
  FolderOpen,
  Search,
  FileSearch,
  Terminal,
  Code,
  Globe,
  Brain,
  Plug,
} from "lucide-svelte";

function makeCall(
  tool_name: string,
  input: Record<string, unknown> = {},
  output: string | null = null,
): ToolCallView {
  return { tool_name, input, output, status: "executed" };
}

// ─── resolveToolDisplay ───────────────────────────────────────────────────────

describe("resolveToolDisplay - native tools", () => {
  test("file_read returns FileText icon and base description key", () => {
    // GIVEN a file_read call without offset
    const call = makeCall("file_read", { path: "src/main.rs" });
    // WHEN
    const info = resolveToolDisplay(call);
    // THEN
    expect(info.icon).toBe(FileText);
    expect(info.labelKey).toBe("tools.labels.file_read");
    expect(info.descriptionKey).toBe("tools.descriptions.file_read");
    expect(info.templateParams.path).toBe("src/main.rs");
  });

  test("file_read with offset uses partial description key", () => {
    // GIVEN a file_read call with offset param
    const call = makeCall("file_read", { path: "src/main.rs", offset: 100 });
    // WHEN
    const info = resolveToolDisplay(call);
    // THEN
    expect(info.descriptionKey).toBe("tools.descriptions.file_read_partial");
  });

  test("file_write returns FilePen icon", () => {
    const call = makeCall("file_write", { path: "out.txt" });
    const info = resolveToolDisplay(call);
    expect(info.icon).toBe(FilePen);
    expect(info.labelKey).toBe("tools.labels.file_write");
    expect(info.descriptionKey).toBe("tools.descriptions.file_write");
  });

  test("file_edit without replace_all uses base description key", () => {
    const call = makeCall("file_edit", { path: "src/lib.rs" });
    const info = resolveToolDisplay(call);
    expect(info.icon).toBe(FilePenLine);
    expect(info.descriptionKey).toBe("tools.descriptions.file_edit");
  });

  test("file_edit with replace_all=true uses replace_all description key", () => {
    const call = makeCall("file_edit", { path: "src/lib.rs", replace_all: true });
    const info = resolveToolDisplay(call);
    expect(info.descriptionKey).toBe("tools.descriptions.file_edit_replace_all");
  });

  test("file_list without recursive uses base description key", () => {
    const call = makeCall("file_list", { dir: "src/" });
    const info = resolveToolDisplay(call);
    expect(info.icon).toBe(FolderOpen);
    expect(info.descriptionKey).toBe("tools.descriptions.file_list");
    expect(info.templateParams.path).toBe("src/");
  });

  test("file_list with recursive=true uses recursive description key", () => {
    const call = makeCall("file_list", { dir: "src/", recursive: true });
    const info = resolveToolDisplay(call);
    expect(info.descriptionKey).toBe("tools.descriptions.file_list_recursive");
  });

  test("file_list falls back to path field when dir is absent", () => {
    const call = makeCall("file_list", { path: "legacy/" });
    const info = resolveToolDisplay(call);
    expect(info.templateParams.path).toBe("legacy/");
  });

  test("file_glob without path uses base description key", () => {
    const call = makeCall("file_glob", { pattern: "**/*.rs" });
    const info = resolveToolDisplay(call);
    expect(info.icon).toBe(Search);
    expect(info.descriptionKey).toBe("tools.descriptions.file_glob");
    expect(info.templateParams.pattern).toBe("**/*.rs");
    expect(info.templateParams.path).toBeUndefined();
  });

  test("file_glob with path uses file_glob_in description key", () => {
    const call = makeCall("file_glob", { pattern: "**/*.rs", path: "crates/" });
    const info = resolveToolDisplay(call);
    expect(info.descriptionKey).toBe("tools.descriptions.file_glob_in");
    expect(info.templateParams.path).toBeDefined();
  });

  test("file_grep without glob uses base description key", () => {
    const call = makeCall("file_grep", { pattern: "fn main" });
    const info = resolveToolDisplay(call);
    expect(info.icon).toBe(FileSearch);
    expect(info.descriptionKey).toBe("tools.descriptions.file_grep");
  });

  test("file_grep with glob uses file_grep_in description key", () => {
    const call = makeCall("file_grep", { pattern: "fn main", glob: "*.rs" });
    const info = resolveToolDisplay(call);
    expect(info.descriptionKey).toBe("tools.descriptions.file_grep_in");
    expect(info.templateParams.glob).toBe("*.rs");
  });

  test("bash_executor returns Terminal icon", () => {
    const call = makeCall("bash_executor", { command: "ls -la" });
    const info = resolveToolDisplay(call);
    expect(info.icon).toBe(Terminal);
    expect(info.templateParams.command).toBe("ls -la");
  });

  test("python_executor returns Code icon", () => {
    const call = makeCall("python_executor", { code: "print('hello')" });
    const info = resolveToolDisplay(call);
    expect(info.icon).toBe(Code);
    expect(info.templateParams.code).toBe("print('hello')");
  });

  test("http_fetch GET uses http_fetch_get description key", () => {
    const call = makeCall("http_fetch", { url: "https://api.example.com/data" });
    const info = resolveToolDisplay(call);
    expect(info.icon).toBe(Globe);
    expect(info.descriptionKey).toBe("tools.descriptions.http_fetch_get");
    expect(info.templateParams.hostname).toBe("api.example.com");
    expect(info.templateParams.method).toBe("GET");
  });

  test("http_fetch POST uses http_fetch_post description key", () => {
    const call = makeCall("http_fetch", {
      url: "https://api.example.com/submit",
      method: "post",
    });
    const info = resolveToolDisplay(call);
    expect(info.descriptionKey).toBe("tools.descriptions.http_fetch_post");
    expect(info.templateParams.method).toBe("POST");
  });

  test("http_fetch with non-GET/POST uses generic description key", () => {
    const call = makeCall("http_fetch", {
      url: "https://api.example.com/res",
      method: "DELETE",
    });
    const info = resolveToolDisplay(call);
    expect(info.descriptionKey).toBe("tools.descriptions.http_fetch");
  });

  test("memory_search without namespace uses base description key", () => {
    const call = makeCall("memory_search", { query: "user preferences" });
    const info = resolveToolDisplay(call);
    expect(info.icon).toBe(Brain);
    expect(info.descriptionKey).toBe("tools.descriptions.memory_search");
  });

  test("memory_search with namespace uses memory_search_ns description key", () => {
    const call = makeCall("memory_search", {
      query: "user preferences",
      namespace: "settings",
    });
    const info = resolveToolDisplay(call);
    expect(info.descriptionKey).toBe("tools.descriptions.memory_search_ns");
    expect(info.templateParams.namespace).toBe("settings");
  });
});

// ─── MCP tools ───────────────────────────────────────────────────────────────

describe("resolveToolDisplay - MCP tools", () => {
  test("mcp:server/tool returns Plug icon with server_name and tool_description", () => {
    // GIVEN a call with MCP tool name
    const call = makeCall("mcp:notion/create_page");
    // WHEN
    const info = resolveToolDisplay(call);
    // THEN
    expect(info.icon).toBe(Plug);
    expect(info.labelKey).toBe("tools.labels.mcp_tool");
    expect(info.templateParams.server_name).toBe("notion");
    expect(info.templateParams.tool_description).toBe("create_page");
  });

  test("mcp tool without slash returns server_name and empty tool_description", () => {
    const call = makeCall("mcp:github");
    const info = resolveToolDisplay(call);
    expect(info.templateParams.server_name).toBe("github");
    expect(info.templateParams.tool_description).toBe("");
  });
});

// ─── Unknown tools ────────────────────────────────────────────────────────────

describe("resolveToolDisplay - unknown tools", () => {
  test("unknown tool returns Terminal icon and dynamic label/description keys", () => {
    // GIVEN an unrecognised tool name
    const call = makeCall("unknown_tool");
    // WHEN
    const info = resolveToolDisplay(call);
    // THEN
    expect(info.icon).toBe(Terminal);
    expect(info.labelKey).toBe("tools.labels.unknown_tool");
    expect(info.descriptionKey).toBe("tools.descriptions.unknown_tool");
    expect(info.outputSummaryKey).toBeNull();
  });
});

// ─── resolveOutputParams ─────────────────────────────────────────────────────

describe("resolveOutputParams", () => {
  test("parses JSON output and converts values to strings", () => {
    // GIVEN a tool call with structured JSON output
    const call = makeCall("file_read", {}, '{"total_lines": 50, "truncated": false}');
    // WHEN
    const params = resolveOutputParams(call);
    // THEN
    expect(params.total_lines).toBe("50");
    expect(params.truncated).toBe("false");
  });

  test("computes count and _count for array fields", () => {
    // GIVEN a tool call with an array field in output
    const call = makeCall("file_list", {}, '{"entries": [{"name":"a"},{"name":"b"},{"name":"c"}]}');
    // WHEN
    const params = resolveOutputParams(call);
    // THEN
    expect(params.entries_count).toBe("3");
    expect(params.count).toBe("3");
  });

  test("returns empty object for null output", () => {
    const call = makeCall("file_read", {}, null);
    expect(resolveOutputParams(call)).toEqual({});
  });

  test("returns empty object for non-object JSON (array)", () => {
    const call = makeCall("file_read", {}, "[1, 2, 3]");
    expect(resolveOutputParams(call)).toEqual({});
  });

  test("returns empty object for invalid JSON", () => {
    const call = makeCall("file_read", {}, "not json");
    expect(resolveOutputParams(call)).toEqual({});
  });

  test("returns empty object for plain string output", () => {
    const call = makeCall("bash_executor", {}, '"just a string"');
    expect(resolveOutputParams(call)).toEqual({});
  });
});

// ─── truncatePath ─────────────────────────────────────────────────────────────

describe("truncatePath", () => {
  test("returns path unchanged when shorter than max", () => {
    expect(truncatePath("src/main.rs")).toBe("src/main.rs");
  });

  test("preserves filename for a long path", () => {
    // GIVEN a path longer than the default 50-char limit
    const longPath = "very/long/nested/path/to/some/deeply/buried/file.rs";
    // WHEN
    const result = truncatePath(longPath);
    // THEN the filename is preserved
    expect(result.endsWith("file.rs")).toBe(true);
    expect(result.length).toBeLessThanOrEqual(50);
    expect(result.startsWith("\u2026")).toBe(true);
  });

  test("respects a custom max parameter", () => {
    const result = truncatePath("a/b/c/d/file.rs", 20);
    expect(result.endsWith("file.rs")).toBe(true);
    expect(result.length).toBeLessThanOrEqual(20);
  });

  test("handles Windows-style backslash paths", () => {
    const result = truncatePath(String.raw`C:\Users\user\Documents\Projects\code\file.ts`, 30);
    expect(result.endsWith("file.ts")).toBe(true);
  });

  test("handles filename longer than max", () => {
    // GIVEN a filename that is itself longer than max - 3
    const result = truncatePath("a/verylongfilenamethatexceedslimit.ts", 15);
    // THEN ellipsis + truncated filename
    expect(result.startsWith("\u2026")).toBe(true);
    expect(result.length).toBeLessThanOrEqual(15);
  });
});

// ─── truncateString ───────────────────────────────────────────────────────────

describe("truncateString", () => {
  test("returns string unchanged when within limit", () => {
    expect(truncateString("hello", 10)).toBe("hello");
  });

  test("truncates and appends ellipsis when over limit", () => {
    const result = truncateString("hello world", 5);
    expect(result).toBe("hello\u2026");
    expect(result.length).toBe(6);
  });

  test("handles exact-length string without truncation", () => {
    expect(truncateString("hello", 5)).toBe("hello");
  });
});

describe("formatRationale", () => {
  test("returns null for nullish input", () => {
    expect(formatRationale(null)).toBeNull();
    expect(formatRationale(undefined)).toBeNull();
  });

  test("truncates long values in inputs_recap and keeps order", () => {
    const r = formatRationale({
      summary: "why",
      inputs_recap: [
        ["path", "a".repeat(80)],
        ["offset", "0"],
      ],
      expected_outcome: "what",
      performance_hint: null,
    });
    expect(r).not.toBeNull();
    expect(r!.inputs_recap.length).toBe(2);
    expect(r!.inputs_recap[0][0]).toBe("path");
    expect(r!.inputs_recap[0][1].length).toBeLessThanOrEqual(41);
  });

  test("caps recap to 4 entries", () => {
    const many: Array<[string, string]> = Array.from({ length: 10 }, (_, i) => [
      `k${i}`,
      `v${i}`,
    ]);
    const r = formatRationale({
      summary: "s",
      inputs_recap: many,
      expected_outcome: "e",
      performance_hint: null,
    });
    expect(r!.inputs_recap.length).toBe(4);
  });

  test("trims performance_hint and drops empty hint", () => {
    expect(
      formatRationale({
        summary: "s",
        inputs_recap: [],
        expected_outcome: "e",
        performance_hint: "   ",
      })!.performance_hint,
    ).toBeNull();
    expect(
      formatRationale({
        summary: "s",
        inputs_recap: [],
        expected_outcome: "e",
        performance_hint: "  200ms  ",
      })!.performance_hint,
    ).toBe("200ms");
  });
});

// ─── Operator readability of an arbitrary tool result ────────────────────────

describe("humanizeToolError - what an operator is shown when a call fails", () => {
  test("GIVEN a runtime-prefixed connector error WHEN humanized THEN only the sentence remains", () => {
    // GIVEN the exact shape the runtime persists
    const raw =
      "tool error: google: no Google account connected - open Settings to sign in";

    // WHEN
    const text = humanizeToolError(raw);

    // THEN
    expect(text).toBe("no Google account connected - open Settings to sign in");
  });

  test("GIVEN an executor error code WHEN humanized THEN the code is dropped", () => {
    expect(humanizeToolError("tool error: spawn_failed: No such file or directory")).toBe(
      "No such file or directory",
    );
  });

  test("GIVEN a plain sentence with a colon WHEN humanized THEN it is left alone", () => {
    // The colon belongs to the sentence, not to a machine token.
    expect(humanizeToolError("Could not reach the server: try again later")).toBe(
      "Could not reach the server: try again later",
    );
  });
});

describe("summariseJsonOutput - a readable shape for tools with no bespoke body", () => {
  test("GIVEN an empty payload WHEN summarised THEN it reads as empty", () => {
    expect(summariseJsonOutput('{"resources":[]}')).toBe("empty");
    expect(summariseJsonOutput("{}")).toBe("empty");
    expect(summariseJsonOutput("[]")).toBe("empty");
  });

  test("GIVEN a populated payload WHEN summarised THEN it reads as a count", () => {
    expect(summariseJsonOutput('{"rules":[{"id":1},{"id":2}]}')).toBe("2");
    expect(summariseJsonOutput('[{"a":1}]')).toBe("1");
  });

  test("GIVEN output with no readable shape WHEN summarised THEN nothing is claimed", () => {
    expect(summariseJsonOutput("not json at all")).toBeNull();
    expect(summariseJsonOutput('{"title":"a page"}')).toBeNull();
  });
});
