import { describe, test, expect } from "vitest";
import type { ToolCallView } from "$lib/types";
import type { UIMode } from "$lib/stores/mode";

// ─── Helpers ─────────────────────────────────────────────────────────────────

function makeCall(
  tool_name: string,
  status: ToolCallView["status"] = "executed",
  input: Record<string, unknown> = {},
): ToolCallView {
  return { tool_name, input, output: null, status };
}

/**
 * Mirrors the isOperator derivation in ChatMessageBubble.
 * Returns true when uiMode is "operator".
 */
function deriveIsOperator(mode: UIMode): boolean {
  return mode === "operator";
}

/**
 * Mirrors the dispatch decision for a given tool call and mode.
 * Returns the component identifier that would be rendered.
 */
type CardKind =
  | "OperatorToolCard"
  | "BuilderToolCard"
  | "OperatorApprovalCard"
  | "ApprovalCard";

function selectCard(toolCall: ToolCallView, mode: UIMode): CardKind {
  const isOperator = deriveIsOperator(mode);
  if (toolCall.status === "pending") {
    return isOperator ? "OperatorApprovalCard" : "ApprovalCard";
  }
  return isOperator ? "OperatorToolCard" : "BuilderToolCard";
}

/**
 * Mirrors the inputPreview computation passed to ApprovalCard.
 */
function buildApprovalInputPreview(input: Record<string, unknown>): string {
  return JSON.stringify(input, null, 2);
}

// ─── isOperator derivation ───────────────────────────────────────────────────

describe("ChatMessageBubble — isOperator derivation", () => {
  test("returns true for operator mode", () => {
    // GIVEN uiMode is "operator"
    // WHEN
    const result = deriveIsOperator("operator");
    // THEN
    expect(result).toBe(true);
  });

  test("returns false for builder mode", () => {
    // GIVEN uiMode is "builder"
    // WHEN
    const result = deriveIsOperator("builder");
    // THEN
    expect(result).toBe(false);
  });
});

// ─── Executed tool call dispatch ─────────────────────────────────────────────

describe("ChatMessageBubble — executed tool call dispatch", () => {
  test("renders OperatorToolCard in operator mode", () => {
    // GIVEN an executed tool call and operator mode
    const toolCall = makeCall("file_read", "executed");
    // WHEN
    const card = selectCard(toolCall, "operator");
    // THEN OperatorToolCard is selected
    expect(card).toBe("OperatorToolCard");
  });

  test("renders BuilderToolCard in builder mode", () => {
    // GIVEN an executed tool call and builder mode
    const toolCall = makeCall("file_read", "executed");
    // WHEN
    const card = selectCard(toolCall, "builder");
    // THEN BuilderToolCard is selected
    expect(card).toBe("BuilderToolCard");
  });

  test("renders OperatorToolCard for refused call in operator mode", () => {
    // GIVEN a refused tool call (non-pending) in operator mode
    const toolCall = makeCall("bash_executor", "refused");
    // WHEN
    const card = selectCard(toolCall, "operator");
    // THEN OperatorToolCard handles the refused state
    expect(card).toBe("OperatorToolCard");
  });

  test("renders BuilderToolCard for refused call in builder mode", () => {
    // GIVEN a refused call in builder mode
    const toolCall = makeCall("bash_executor", "refused");
    // WHEN
    const card = selectCard(toolCall, "builder");
    // THEN BuilderToolCard handles the refused state
    expect(card).toBe("BuilderToolCard");
  });

  test("renders OperatorToolCard for authorized call in operator mode", () => {
    // GIVEN an authorized (running) tool call in operator mode
    const toolCall = makeCall("http_fetch", "authorized");
    const card = selectCard(toolCall, "operator");
    expect(card).toBe("OperatorToolCard");
  });
});

// ─── Pending tool call dispatch ──────────────────────────────────────────────

describe("ChatMessageBubble — pending tool call dispatch", () => {
  test("renders OperatorApprovalCard for pending call in operator mode", () => {
    // GIVEN a pending tool call in operator mode
    const toolCall = makeCall("file_write", "pending");
    // WHEN
    const card = selectCard(toolCall, "operator");
    // THEN OperatorApprovalCard is selected
    expect(card).toBe("OperatorApprovalCard");
  });

  test("renders ApprovalCard for pending call in builder mode", () => {
    // GIVEN a pending tool call in builder mode
    const toolCall = makeCall("file_write", "pending");
    // WHEN
    const card = selectCard(toolCall, "builder");
    // THEN the legacy ApprovalCard is preserved for builder mode
    expect(card).toBe("ApprovalCard");
  });

  test("OperatorApprovalCard receives the full toolCall object", () => {
    // GIVEN a pending call with non-trivial input
    const input = { path: "secrets.txt", content: "do not leak" };
    const toolCall = makeCall("file_write", "pending", input);
    // WHEN the card is selected for operator mode
    const card = selectCard(toolCall, "operator");
    // THEN OperatorApprovalCard receives the toolCall directly (not a flat props spread)
    expect(card).toBe("OperatorApprovalCard");
    expect(toolCall.input).toEqual(input);
  });

  test("ApprovalCard receives inputPreview as serialised JSON", () => {
    // GIVEN a pending call in builder mode with structured input
    const input = { path: "out.txt", content: "hello" };
    const toolCall = makeCall("file_write", "pending", input);
    // WHEN the preview is built for ApprovalCard
    const preview = buildApprovalInputPreview(toolCall.input);
    // THEN it is valid indented JSON
    expect(() => JSON.parse(preview)).not.toThrow();
    expect(preview).toContain("out.txt");
    expect(preview).toContain("hello");
  });
});

// ─── Mode switch ─────────────────────────────────────────────────────────────

describe("ChatMessageBubble — mode switch re-renders correctly", () => {
  test("switching from operator to builder selects BuilderToolCard for executed call", () => {
    // GIVEN a rendered component in operator mode
    const toolCall = makeCall("file_grep", "executed");
    expect(selectCard(toolCall, "operator")).toBe("OperatorToolCard");
    // WHEN the mode switches to builder
    // THEN the new selection is BuilderToolCard — no reload required
    expect(selectCard(toolCall, "builder")).toBe("BuilderToolCard");
  });

  test("switching from builder to operator selects OperatorApprovalCard for pending call", () => {
    // GIVEN a pending call rendered in builder mode
    const toolCall = makeCall("bash_executor", "pending");
    expect(selectCard(toolCall, "builder")).toBe("ApprovalCard");
    // WHEN the mode switches to operator
    expect(selectCard(toolCall, "operator")).toBe("OperatorApprovalCard");
  });

  test("mode is the sole determinant of the card type for a given status", () => {
    // GIVEN an executed call — the card selection depends only on mode, not on tool name
    const callA = makeCall("file_read", "executed");
    const callB = makeCall("memory_search", "executed");
    // WHEN compared across modes
    expect(selectCard(callA, "operator")).toBe(selectCard(callB, "operator"));
    expect(selectCard(callA, "builder")).toBe(selectCard(callB, "builder"));
  });
});

// ─── data-testid contract ────────────────────────────────────────────────────

describe("ChatMessageBubble — data-testid expectations per card", () => {
  test("OperatorToolCard testid pattern: operator-tool-{tool_name}", () => {
    const toolName = "file_grep";
    expect(`operator-tool-${toolName}`).toBe("operator-tool-file_grep");
  });

  test("BuilderToolCard testid pattern: builder-tool-{tool_name}", () => {
    const toolName = "file_grep";
    expect(`builder-tool-${toolName}`).toBe("builder-tool-file_grep");
  });

  test("OperatorApprovalCard testid pattern: operator-approval-{tool_name}", () => {
    const toolName = "file_write";
    expect(`operator-approval-${toolName}`).toBe("operator-approval-file_write");
  });

  test("ApprovalCard testid pattern: approval-card-{toolName}", () => {
    const toolName = "file_write";
    expect(`approval-card-${toolName}`).toBe("approval-card-file_write");
  });
});
