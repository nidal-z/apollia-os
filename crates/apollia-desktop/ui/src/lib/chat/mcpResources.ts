/**
 * MCP resource @-mention support for the chat input (user path).
 *
 * The agent-initiative path lives in the runtime (`mcp_resources_list` /
 * `mcp_resources_read` tools). This module backs the complementary
 * user-initiative path: when the user types `@`, a picker lists the MCP
 * resources exposed by connected servers. Selecting one PINS it; on the next
 * turn the pinned resources are prepended as an explicit system-prefix block.
 * Nothing is ever injected automatically.
 */
import { invoke } from "@tauri-apps/api/core";

/** One MCP resource entry, as returned by the `list_mcp_resources` command. */
export interface McpResourceView {
  /** Owning MCP server name. */
  server: string;
  /** Stable URI identifying the resource. */
  uri: string;
  /** Display name. */
  name: string;
  /** MIME type when known. */
  mime_type?: string | null;
  /** Optional one-line description. */
  description?: string | null;
}

/** A resource the user has pinned to the next turn. */
export interface PinnedResource {
  server: string;
  uri: string;
  name: string;
}

/**
 * Detect a trailing `@`-mention token the user is currently typing.
 *
 * Returns the query text after the most recent `@` when the cursor is inside an
 * unbroken mention token (no whitespace since the `@`), otherwise `null`. The
 * `@` must start the input or follow whitespace, so email addresses and inline
 * `foo@bar` do not trigger the picker.
 */
export function detectMentionQuery(value: string, cursor: number): string | null {
  const upto = value.slice(0, cursor);
  const at = upto.lastIndexOf("@");
  if (at < 0) return null;
  const before = at === 0 ? "" : upto[at - 1];
  if (before !== "" && !/\s/.test(before)) return null;
  const token = upto.slice(at + 1);
  if (/\s/.test(token)) return null;
  return token;
}

/** Filter resources by a case-insensitive substring of name, uri, or server. */
export function filterResources(
  resources: McpResourceView[],
  query: string,
): McpResourceView[] {
  const q = query.trim().toLowerCase();
  if (q === "") return resources;
  return resources.filter(
    (r) =>
      r.name.toLowerCase().includes(q) ||
      r.uri.toLowerCase().includes(q) ||
      r.server.toLowerCase().includes(q),
  );
}

/**
 * Fetch the aggregated MCP resource list from the runtime.
 *
 * Returns an empty array (never throws) when MCP is not configured or the call
 * fails, so the picker degrades to "no resources" instead of erroring.
 */
export async function fetchMcpResources(): Promise<McpResourceView[]> {
  try {
    return await invoke<McpResourceView[]>("list_mcp_resources");
  } catch (err) {
    console.warn("list_mcp_resources failed", err);
    return [];
  }
}

/**
 * Build the system-prefix block prepended to the user message when resources
 * are pinned. The block is an explicit, user-triggered context injection: the
 * agent receives the pinned URIs and is told it may read them with the
 * `mcp_resources_read` tool. The content itself is NOT inlined here (the agent
 * reads on demand), keeping token cost bounded and principle #6 intact.
 */
export function buildPinnedPrefix(pinned: PinnedResource[]): string {
  if (pinned.length === 0) return "";
  const lines = pinned.map(
    (p) => `- ${p.name} (server="${p.server}", uri="${p.uri}")`,
  );
  return [
    "<pinned-mcp-resources>",
    "The user pinned the following MCP resources for this turn.",
    "Read any you need with the mcp_resources_read tool.",
    ...lines,
    "</pinned-mcp-resources>",
    "",
  ].join("\n");
}
