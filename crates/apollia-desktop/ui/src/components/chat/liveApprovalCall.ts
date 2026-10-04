/**
 * The tool call a live approval card describes.
 *
 * A live approval arrives as a `ChatApprovalRequired` event, which carries the
 * tool name and a prompt of the form `Tool '<name>' asks to run with: <json>`,
 * not the tool call itself. The default approval card reads a `ToolCallView`,
 * so the arguments are recovered from that prompt when they parse, and left
 * empty otherwise: the card then describes the tool by its name alone.
 */
import type { ToolCallView } from "$lib/types";

const PROMPT_MARKER = "asks to run with: ";

function asArguments(text: string): Record<string, unknown> | null {
  try {
    const parsed: unknown = JSON.parse(text);
    if (typeof parsed === "object" && parsed !== null && !Array.isArray(parsed)) {
      return parsed as Record<string, unknown>;
    }
  } catch {
    // Truncated or free text: no arguments to show.
  }
  return null;
}

/** Builds the pending tool call of a live approval from its event fields. */
export function liveApprovalCall(toolName: string, inputPreview: string): ToolCallView {
  const marker = inputPreview.indexOf(PROMPT_MARKER);
  const input =
    asArguments(inputPreview) ??
    (marker >= 0 ? asArguments(inputPreview.slice(marker + PROMPT_MARKER.length)) : null) ??
    {};
  return { tool_name: toolName, input, output: null, status: "pending" };
}
