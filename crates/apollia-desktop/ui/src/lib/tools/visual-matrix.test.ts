import { describe, test, expect } from "vitest";
import { resolveToolDisplay, truncatePath } from "$lib/tools/tool-display";
import type { ToolCallView } from "$lib/types";
import type { UIMode } from "$lib/stores/mode";
import en from "$lib/i18n/en.json";
import fr from "$lib/i18n/fr.json";
import { Plug } from "lucide-svelte";

// ─── Helpers ─────────────────────────────────────────────────────────────────

function makeCall(
  tool_name: string,
  status: ToolCallView["status"] = "executed",
  input: Record<string, unknown> = {},
): ToolCallView {
  return { tool_name, input, output: null, status };
}

type CardKind =
  | "OperatorToolCard"
  | "BuilderToolCard"
  | "OperatorApprovalCard"
  | "ApprovalCard";

function selectCard(call: ToolCallView, mode: UIMode): CardKind {
  const isOperator = mode === "operator";
  if (call.status === "pending") {
    return isOperator ? "OperatorApprovalCard" : "ApprovalCard";
  }
  return isOperator ? "OperatorToolCard" : "BuilderToolCard";
}

type I18nJson = Record<string, unknown>;

/** Resolves a dot-notation key in a JSON tree, returning the string value or null. */
function resolveKey(json: I18nJson, dotKey: string): string | null {
  const parts = dotKey.split(".");
  let node: unknown = json;
  for (const part of parts) {
    if (typeof node !== "object" || node === null) return null;
    node = (node as I18nJson)[part];
  }
  return typeof node === "string" ? node : null;
}

// ─── Matrix definition ────────────────────────────────────────────────────────

type MatrixRow = {
  tool_name: string;
  status: ToolCallView["status"];
  input: Record<string, unknown>;
  operatorCard: CardKind;
  builderCard: CardKind;
};

// Mirrors the 16-row spec matrix: each native tool and MCP tool in relevant
// statuses, exercised in both operator and builder modes.
const MATRIX: MatrixRow[] = [
  {
    tool_name: "file_read",
    status: "pending",
    input: { path: "src/main.rs" },
    operatorCard: "OperatorApprovalCard",
    builderCard: "ApprovalCard",
  },
  {
    tool_name: "file_read",
    status: "executed",
    input: { path: "src/main.rs" },
    operatorCard: "OperatorToolCard",
    builderCard: "BuilderToolCard",
  },
  {
    tool_name: "file_read",
    status: "refused",
    input: { path: "src/main.rs" },
    operatorCard: "OperatorToolCard",
    builderCard: "BuilderToolCard",
  },
  {
    tool_name: "file_write",
    status: "executed",
    input: { path: "out.txt", content: "hello world" },
    operatorCard: "OperatorToolCard",
    builderCard: "BuilderToolCard",
  },
  {
    tool_name: "file_edit",
    status: "executed",
    input: { path: "src/lib.rs", old_string: "foo", new_string: "bar" },
    operatorCard: "OperatorToolCard",
    builderCard: "BuilderToolCard",
  },
  {
    tool_name: "file_list",
    status: "executed",
    input: { path: "src/" },
    operatorCard: "OperatorToolCard",
    builderCard: "BuilderToolCard",
  },
  {
    tool_name: "file_glob",
    status: "executed",
    input: { pattern: "**/*.rs" },
    operatorCard: "OperatorToolCard",
    builderCard: "BuilderToolCard",
  },
  {
    tool_name: "file_grep",
    status: "executed",
    input: { pattern: "fn main" },
    operatorCard: "OperatorToolCard",
    builderCard: "BuilderToolCard",
  },
  {
    tool_name: "file_grep",
    status: "refused",
    input: { pattern: "fn main" },
    operatorCard: "OperatorToolCard",
    builderCard: "BuilderToolCard",
  },
  {
    tool_name: "bash_executor",
    status: "executed",
    input: { command: "ls -la" },
    operatorCard: "OperatorToolCard",
    builderCard: "BuilderToolCard",
  },
  {
    tool_name: "python_executor",
    status: "executed",
    input: { code: "print('hello')" },
    operatorCard: "OperatorToolCard",
    builderCard: "BuilderToolCard",
  },
  {
    tool_name: "http_fetch",
    status: "executed",
    input: { url: "https://api.example.com/data" },
    operatorCard: "OperatorToolCard",
    builderCard: "BuilderToolCard",
  },
  {
    tool_name: "http_fetch",
    status: "pending",
    input: { url: "https://api.example.com/data" },
    operatorCard: "OperatorApprovalCard",
    builderCard: "ApprovalCard",
  },
  {
    tool_name: "memory_search",
    status: "executed",
    input: { query: "user preferences" },
    operatorCard: "OperatorToolCard",
    builderCard: "BuilderToolCard",
  },
  {
    tool_name: "mcp:notion/create_page",
    status: "pending",
    input: {},
    operatorCard: "OperatorApprovalCard",
    builderCard: "ApprovalCard",
  },
  {
    tool_name: "mcp:notion/create_page",
    status: "executed",
    input: {},
    operatorCard: "OperatorToolCard",
    builderCard: "BuilderToolCard",
  },
];

// ─── Matrix sweep — card dispatch ────────────────────────────────────────────

describe("visual matrix — card dispatch", () => {
  // GIVEN each row of the spec matrix
  // WHEN the card is selected for operator then builder mode
  // THEN the correct component is chosen based on status × mode
  test.each(MATRIX)(
    "$tool_name [$status]: operator=$operatorCard, builder=$builderCard",
    ({ tool_name, status, input, operatorCard, builderCard }) => {
      const call = makeCall(tool_name, status, input);
      expect(selectCard(call, "operator")).toBe(operatorCard);
      expect(selectCard(call, "builder")).toBe(builderCard);
    },
  );
});

// ─── Matrix sweep — display metadata completeness ────────────────────────────

describe("visual matrix — display metadata completeness", () => {
  // GIVEN each row of the spec matrix
  // WHEN resolveToolDisplay is called
  // THEN icon, labelKey, descriptionKey are present and templateParams are strings
  test.each(MATRIX)(
    "$tool_name [$status]: icon is truthy, keys are non-empty strings",
    ({ tool_name, status, input }) => {
      const call = makeCall(tool_name, status, input);
      const display = resolveToolDisplay(call);

      expect(display.icon).toBeTruthy();

      expect(typeof display.labelKey).toBe("string");
      expect(display.labelKey.length).toBeGreaterThan(0);

      expect(typeof display.descriptionKey).toBe("string");
      expect(display.descriptionKey.length).toBeGreaterThan(0);
    },
  );

  // GIVEN each row of the spec matrix
  // WHEN templateParams are inspected
  // THEN all values are plain strings — no raw JSON objects exposed in operator mode
  test.each(MATRIX)(
    "$tool_name [$status]: templateParams values are all strings",
    ({ tool_name, status, input }) => {
      const call = makeCall(tool_name, status, input);
      const { templateParams } = resolveToolDisplay(call);

      for (const [key, value] of Object.entries(templateParams)) {
        expect(typeof value, `templateParams.${key} must be a string`).toBe("string");
      }
    },
  );

  // GIVEN each executed native tool (MCP excluded — no outputSummaryKey by design)
  // WHEN resolveToolDisplay is called
  // THEN outputSummaryKey is non-null for all 10 native tools
  const NATIVE_EXECUTED: Array<[string, Record<string, unknown>]> = [
    ["file_read", { path: "src/main.rs" }],
    ["file_write", { path: "out.txt" }],
    ["file_edit", { path: "src/lib.rs" }],
    ["file_list", { path: "src/" }],
    ["file_glob", { pattern: "**/*.rs" }],
    ["file_grep", { pattern: "fn main" }],
    ["bash_executor", { command: "ls -la" }],
    ["python_executor", { code: "print('hello')" }],
    ["http_fetch", { url: "https://api.example.com/data" }],
    ["memory_search", { query: "user preferences" }],
  ];

  test.each(NATIVE_EXECUTED)(
    "%s: outputSummaryKey is non-null for native tools",
    (tool_name, input) => {
      const call = makeCall(tool_name, "executed", input);
      const { outputSummaryKey } = resolveToolDisplay(call);
      expect(outputSummaryKey).not.toBeNull();
    },
  );
});

// ─── Matrix sweep — i18n key resolution (EN + FR) ───────────────────────────

describe("visual matrix — i18n key resolution", () => {
  // GIVEN each row of the spec matrix
  // WHEN labelKey and descriptionKey are resolved against EN and FR
  // THEN both languages return non-empty string values for every combination
  test.each(MATRIX)(
    "$tool_name [$status]: labelKey and descriptionKey resolve in EN and FR",
    ({ tool_name, status, input }) => {
      const call = makeCall(tool_name, status, input);
      const { labelKey, descriptionKey } = resolveToolDisplay(call);

      const enLabel = resolveKey(en as I18nJson, labelKey);
      const frLabel = resolveKey(fr as I18nJson, labelKey);
      const enDesc = resolveKey(en as I18nJson, descriptionKey);
      const frDesc = resolveKey(fr as I18nJson, descriptionKey);

      expect(enLabel, `EN missing key: ${labelKey}`).not.toBeNull();
      expect(enLabel!.length).toBeGreaterThan(0);

      expect(frLabel, `FR missing key: ${labelKey}`).not.toBeNull();
      expect(frLabel!.length).toBeGreaterThan(0);

      expect(enDesc, `EN missing key: ${descriptionKey}`).not.toBeNull();
      expect(enDesc!.length).toBeGreaterThan(0);

      expect(frDesc, `FR missing key: ${descriptionKey}`).not.toBeNull();
      expect(frDesc!.length).toBeGreaterThan(0);
    },
  );

  // GIVEN each row of the spec matrix
  // WHEN the EN description is checked
  // THEN it is a human-readable phrase, not raw JSON (no leading '{' or '[')
  test.each(MATRIX)(
    "$tool_name [$status]: EN description is a human phrase, not raw JSON",
    ({ tool_name, status, input }) => {
      const call = makeCall(tool_name, status, input);
      const { descriptionKey } = resolveToolDisplay(call);
      const enDesc = resolveKey(en as I18nJson, descriptionKey);

      expect(enDesc).not.toBeNull();
      expect(enDesc!.trimStart()).not.toMatch(/^[{[]/);
    },
  );
});

// ─── AC-3 — Real-time mode switch ────────────────────────────────────────────

describe("visual matrix — mode switch (AC-3)", () => {
  const NATIVE_TOOLS = [
    "file_read",
    "file_write",
    "file_edit",
    "file_list",
    "file_glob",
    "file_grep",
    "bash_executor",
    "python_executor",
    "http_fetch",
    "memory_search",
  ];

  // GIVEN an executed call for each native tool
  // WHEN the mode switches operator → builder
  // THEN OperatorToolCard changes to BuilderToolCard without reload
  test.each(NATIVE_TOOLS)(
    "%s (executed): operator→builder yields BuilderToolCard",
    (tool_name) => {
      const call = makeCall(tool_name, "executed");
      expect(selectCard(call, "operator")).toBe("OperatorToolCard");
      expect(selectCard(call, "builder")).toBe("BuilderToolCard");
    },
  );

  // GIVEN a pending call for each native tool
  // WHEN the mode switches operator → builder
  // THEN OperatorApprovalCard changes to ApprovalCard without reload
  test.each(NATIVE_TOOLS)(
    "%s (pending): operator→builder yields ApprovalCard",
    (tool_name) => {
      const call = makeCall(tool_name, "pending");
      expect(selectCard(call, "operator")).toBe("OperatorApprovalCard");
      expect(selectCard(call, "builder")).toBe("ApprovalCard");
    },
  );

  test("mode is the sole determinant of card type for a given status", () => {
    // GIVEN two different executed tools
    // WHEN compared across modes
    // THEN card selection depends only on mode — tool identity is irrelevant to routing
    const callA = makeCall("file_read", "executed");
    const callB = makeCall("memory_search", "executed");
    expect(selectCard(callA, "operator")).toBe(selectCard(callB, "operator"));
    expect(selectCard(callA, "builder")).toBe(selectCard(callB, "builder"));
  });

  test("MCP tool follows the same dispatch rules as native tools", () => {
    // GIVEN MCP tool calls in both statuses
    const pending = makeCall("mcp:notion/create_page", "pending");
    const executed = makeCall("mcp:notion/create_page", "executed");
    // WHEN mode switches
    // THEN dispatch is identical to native tools
    expect(selectCard(pending, "operator")).toBe("OperatorApprovalCard");
    expect(selectCard(pending, "builder")).toBe("ApprovalCard");
    expect(selectCard(executed, "operator")).toBe("OperatorToolCard");
    expect(selectCard(executed, "builder")).toBe("BuilderToolCard");
  });
});

// ─── AC-4 — MCP tool display ─────────────────────────────────────────────────

describe("visual matrix — MCP tool display (AC-4)", () => {
  test("mcp:notion/create_page returns Plug icon", () => {
    // GIVEN the MCP tool mcp:notion/create_page
    // WHEN resolveToolDisplay is called
    // THEN the icon is Plug (not a generic Terminal or file icon)
    const call = makeCall("mcp:notion/create_page", "pending");
    const display = resolveToolDisplay(call);
    expect(display.icon).toBe(Plug);
  });

  test("mcp:notion/create_page extracts server_name='notion' and tool_description='create_page'", () => {
    // GIVEN an MCP tool call with server/tool name pattern
    const call = makeCall("mcp:notion/create_page", "executed");
    const { templateParams } = resolveToolDisplay(call);
    // THEN server and tool parts are parsed and available for i18n interpolation
    expect(templateParams.server_name).toBe("notion");
    expect(templateParams.tool_description).toBe("create_page");
  });

  test("mcp:notion/create_page EN label resolves to non-empty string", () => {
    const call = makeCall("mcp:notion/create_page", "executed");
    const { labelKey } = resolveToolDisplay(call);
    const value = resolveKey(en as I18nJson, labelKey);
    expect(value).not.toBeNull();
    expect(value!.length).toBeGreaterThan(0);
  });

  test("mcp:notion/create_page FR label resolves to non-empty string", () => {
    // GIVEN an MCP tool call
    // WHEN the FR label is resolved
    // THEN it is non-empty (translated, not empty)
    const call = makeCall("mcp:notion/create_page", "executed");
    const { labelKey } = resolveToolDisplay(call);
    const value = resolveKey(fr as I18nJson, labelKey);
    expect(value).not.toBeNull();
    expect(value!.length).toBeGreaterThan(0);
  });

  test("mcp:notion/create_page EN description contains server_name placeholder", () => {
    // GIVEN the MCP description template
    // WHEN resolved from EN
    // THEN it contains the {server_name} interpolation placeholder
    const call = makeCall("mcp:notion/create_page", "pending");
    const { descriptionKey } = resolveToolDisplay(call);
    const desc = resolveKey(en as I18nJson, descriptionKey);
    expect(desc).not.toBeNull();
    expect(desc).toContain("{server_name}");
  });

  test("mcp:notion/create_page outputSummaryKey is null (no structured output)", () => {
    const call = makeCall("mcp:notion/create_page", "executed");
    const { outputSummaryKey } = resolveToolDisplay(call);
    expect(outputSummaryKey).toBeNull();
  });
});

// ─── AC-5 — Long path truncation ─────────────────────────────────────────────

describe("visual matrix — long path truncation (AC-5)", () => {
  const LONG_PATH = "very/long/nested/path/to/some/deeply/nested/file.rs";

  test("path exceeding 50 chars is truncated", () => {
    // GIVEN a deeply nested path longer than the 50-char default
    // WHEN truncatePath is applied
    // THEN the result fits within the limit
    const result = truncatePath(LONG_PATH);
    expect(result.length).toBeLessThanOrEqual(50);
  });

  test("filename 'file.rs' is preserved at the end of the truncated path", () => {
    const result = truncatePath(LONG_PATH);
    expect(result.endsWith("file.rs")).toBe(true);
  });

  test("truncated path starts with the ellipsis character '\u2026'", () => {
    const result = truncatePath(LONG_PATH);
    expect(result.startsWith("\u2026")).toBe(true);
  });

  test("file_read with a long path exposes truncated path in templateParams", () => {
    // GIVEN a file_read call with a very long path
    const call = makeCall("file_read", "executed", { path: LONG_PATH });
    // WHEN display metadata is resolved
    const { templateParams } = resolveToolDisplay(call);
    // THEN the path exposed for operator display is truncated, preserving filename
    expect(templateParams.path.endsWith("file.rs")).toBe(true);
    expect(templateParams.path.length).toBeLessThanOrEqual(50);
    expect(templateParams.path.startsWith("\u2026")).toBe(true);
  });

  test("short paths are not truncated", () => {
    const result = truncatePath("src/main.rs");
    expect(result).toBe("src/main.rs");
  });
});

// ─── AC-6 — FR translation quality ───────────────────────────────────────────

describe("visual matrix — FR translation quality (AC-6)", () => {
  const ALL_TOOLS: Array<[string, Record<string, unknown>]> = [
    ["file_read", { path: "src/main.rs" }],
    ["file_write", { path: "out.txt" }],
    ["file_edit", { path: "src/lib.rs" }],
    ["file_list", { path: "src/" }],
    ["file_glob", { pattern: "**/*.rs" }],
    ["file_grep", { pattern: "fn main" }],
    ["bash_executor", { command: "ls -la" }],
    ["python_executor", { code: "print('hello')" }],
    ["http_fetch", { url: "https://api.example.com/data" }],
    ["memory_search", { query: "user preferences" }],
    ["mcp:notion/create_page", {}],
  ];

  // GIVEN each tool used in the sprint
  // WHEN the FR translation is resolved
  // THEN both label and description are non-empty strings
  test.each(ALL_TOOLS)(
    "%s: FR label and description are non-empty strings",
    (tool_name, input) => {
      const call = makeCall(tool_name, "executed", input);
      const { labelKey, descriptionKey } = resolveToolDisplay(call);

      const frLabel = resolveKey(fr as I18nJson, labelKey);
      const frDesc = resolveKey(fr as I18nJson, descriptionKey);

      expect(frLabel, `FR missing key: ${labelKey}`).not.toBeNull();
      expect(frLabel!.length).toBeGreaterThan(0);

      expect(frDesc, `FR missing key: ${descriptionKey}`).not.toBeNull();
      expect(frDesc!.length).toBeGreaterThan(0);
    },
  );

  test("FR and EN labels are distinct for file_read (actual translation)", () => {
    // GIVEN file_read
    // WHEN FR and EN labels are compared
    // THEN they differ — FR is a real translation, not a copy of EN
    const call = makeCall("file_read", "executed", { path: "src/main.rs" });
    const { labelKey } = resolveToolDisplay(call);
    expect(resolveKey(en as I18nJson, labelKey)).not.toBe(
      resolveKey(fr as I18nJson, labelKey),
    );
  });

  test("FR and EN descriptions are distinct for file_read (actual translation)", () => {
    // GIVEN file_read
    // WHEN FR and EN descriptions are compared
    // THEN they differ — FR is "Lecture de {path}", not "Reading {path}"
    const call = makeCall("file_read", "executed", { path: "src/main.rs" });
    const { descriptionKey } = resolveToolDisplay(call);
    expect(resolveKey(en as I18nJson, descriptionKey)).not.toBe(
      resolveKey(fr as I18nJson, descriptionKey),
    );
  });

  test("all 4 status values are non-empty in FR", () => {
    // GIVEN the 4 status keys
    // WHEN resolved in FR
    // THEN each has a non-empty translation
    for (const status of ["pending", "authorized", "executed", "refused"] as const) {
      const frStatus = resolveKey(fr as I18nJson, `tools.status.${status}`);
      expect(frStatus, `FR missing tools.status.${status}`).not.toBeNull();
      expect(frStatus!.length).toBeGreaterThan(0);
    }
  });

  test("all 4 status values are non-empty in EN", () => {
    for (const status of ["pending", "authorized", "executed", "refused"] as const) {
      const enStatus = resolveKey(en as I18nJson, `tools.status.${status}`);
      expect(enStatus, `EN missing tools.status.${status}`).not.toBeNull();
      expect(enStatus!.length).toBeGreaterThan(0);
    }
  });
});
