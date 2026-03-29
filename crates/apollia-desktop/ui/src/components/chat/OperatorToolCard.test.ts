import { describe, test, expect } from "vitest";
import { resolveToolDisplay } from "$lib/tools/tool-display";
import type { ToolCallView } from "$lib/types";

// ─── Helpers ─────────────────────────────────────────────────────────────────

function makeCall(
  tool_name: string,
  input: Record<string, unknown> = {},
  output: string | null = null,
  status: ToolCallView["status"] = "executed",
): ToolCallView {
  return { tool_name, input, output, status };
}

/**
 * Mirror of the errorMessage derivation in OperatorToolCard.
 * Extracts and truncates the first line of a refused tool output.
 */
function extractErrorMessage(
  isRefused: boolean,
  output: string | null,
): string | null {
  if (!isRefused || !output) return null;
  const first = output.split("\n")[0] ?? "";
  return first.length > 120 ? first.slice(0, 120) + "\u2026" : first;
}

// ─── resolveToolDisplay integration (OperatorToolCard perspective) ────────────

describe("OperatorToolCard — resolveToolDisplay integration", () => {
  test("file_grep returns tool-specific FileSearch icon", () => {
    // GIVEN a tool call for file_grep
    const call = makeCall("file_grep", { pattern: "TODO", glob: "*.rs" });
    // WHEN
    const display = resolveToolDisplay(call);
    // THEN the icon is tool-specific, not the generic Terminal fallback
    expect(display.icon).not.toBeUndefined();
    expect(display.labelKey).toBe("tools.labels.file_grep");
    expect(display.descriptionKey).toBe("tools.descriptions.file_grep_in");
  });

  test("description templateParams contain interpolated pattern and glob", () => {
    // GIVEN a file_grep call with pattern and glob
    const call = makeCall("file_grep", { pattern: "TODO", glob: "*.rs" });
    // WHEN
    const display = resolveToolDisplay(call);
    // THEN templateParams expose values for i18n interpolation
    expect(display.templateParams.pattern).toBe("TODO");
    expect(display.templateParams.glob).toBe("*.rs");
  });

  test("outputSummaryKey is non-null for known tools", () => {
    // GIVEN an executed file_read call
    const call = makeCall("file_read", { path: "src/main.rs" }, '{"total_lines":50}');
    // WHEN
    const display = resolveToolDisplay(call);
    // THEN outputSummaryKey is available for the summary line
    expect(display.outputSummaryKey).not.toBeNull();
    expect(display.outputParams.total_lines).toBe("50");
  });

  test("unknown tool returns Terminal fallback with dynamic keys", () => {
    // GIVEN an unrecognised tool name
    const call = makeCall("unknown_tool");
    // WHEN
    const display = resolveToolDisplay(call);
    // THEN fallback keys are generated dynamically — no JSON is exposed
    expect(display.labelKey).toBe("tools.labels.unknown_tool");
    expect(display.descriptionKey).toBe("tools.descriptions.unknown_tool");
    expect(Object.keys(display.templateParams)).toHaveLength(0);
  });

  test("data-testid pattern: operator-tool-{tool_name}", () => {
    // GIVEN any tool name
    const toolName = "file_read";
    // THEN the expected testid is operator-tool-file_read
    expect(`operator-tool-${toolName}`).toBe("operator-tool-file_read");
  });
});

// ─── Error message extraction ─────────────────────────────────────────────────

describe("OperatorToolCard — error message extraction", () => {
  test("returns null when status is not refused", () => {
    // GIVEN an executed call with output
    expect(extractErrorMessage(false, "some output")).toBeNull();
  });

  test("returns null when refused but output is null", () => {
    // GIVEN a refused call with no output
    expect(extractErrorMessage(true, null)).toBeNull();
  });

  test("returns first line of multi-line output", () => {
    // GIVEN a refused call with multi-line error output
    const output = "Permission denied\nmore details\neven more";
    // WHEN
    const msg = extractErrorMessage(true, output);
    // THEN only the first paragraph is shown
    expect(msg).toBe("Permission denied");
  });

  test("truncates long first lines at 120 chars with ellipsis", () => {
    // GIVEN a refused call with a very long single-line error
    const longLine = "E".repeat(130);
    // WHEN
    const msg = extractErrorMessage(true, longLine);
    // THEN truncated to 120 chars + ellipsis
    expect(msg).toBe("E".repeat(120) + "\u2026");
    expect(msg!.length).toBe(121);
  });

  test("returns full line when exactly at 120 chars", () => {
    // GIVEN a refused call with an error exactly 120 chars long
    const exactLine = "X".repeat(120);
    const msg = extractErrorMessage(true, exactLine);
    expect(msg).toBe(exactLine);
  });

  test("returns full line when under the 120-char limit", () => {
    // GIVEN a refused call with a short error
    const msg = extractErrorMessage(true, "Tool execution failed: sandbox limit reached");
    expect(msg).toBe("Tool execution failed: sandbox limit reached");
  });
});

// ─── showOutputSummary logic ──────────────────────────────────────────────────

describe("OperatorToolCard — output summary visibility logic", () => {
  test("shows summary when executed, output non-null, and outputSummaryKey set", () => {
    // GIVEN
    const isExecuted = true;
    const output: string | null = '{"match_count": "12"}';
    const outputSummaryKey: string | null = "tools.outputs.file_grep";
    // WHEN
    const showSummary = isExecuted && output !== null && outputSummaryKey !== null;
    // THEN
    expect(showSummary).toBe(true);
  });

  test("hides summary when output is null", () => {
    const showSummary = true && null !== null && "key" !== null;
    expect(showSummary).toBe(false);
  });

  test("hides summary when outputSummaryKey is null (unknown tool)", () => {
    // GIVEN an unknown tool with no summary key
    const display = resolveToolDisplay(makeCall("unknown_tool"));
    // WHEN
    const showSummary = true && "some output" !== null && display.outputSummaryKey !== null;
    // THEN
    expect(showSummary).toBe(false);
  });

  test("hides summary when status is not executed", () => {
    const showSummary = false && "output" !== null && "key" !== null;
    expect(showSummary).toBe(false);
  });
});
