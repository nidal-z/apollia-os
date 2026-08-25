/**
 * Typed Tauri IPC wrappers for tool introspection.
 *
 * Thin wrappers over the `#[tauri::command]` functions in
 * `crates/apollia-desktop/src/commands/tools.rs`. Components call these instead
 * of `invoke()` directly, so command names and payload shapes live in a single
 * place.
 */
import { invoke } from "@tauri-apps/api/core";
import type { ToolSummary } from "$lib/types";

/**
 * Full contract of a tool, as returned by `describe_tool`.
 *
 * `kind`, `input_schema` and `output_schema` carry raw backend payloads: they
 * are intentionally typed `unknown` and narrowed here rather than being given a
 * fake shape.
 */
export interface ToolDescriptor {
  name: string;
  version: string;
  description: string;
  /** Raw `ToolKind` payload. Read it through {@link toolKindTag}. */
  kind: unknown;
  input_schema: unknown;
  output_schema: unknown;
  permissions: string[];
}

/**
 * Extracts the discriminant of a `ToolKind` payload.
 *
 * The Rust `ToolKind` enum is internally tagged (`#[serde(tag = "type")]`), so
 * the serialized form is an object such as `{ "type": "native" }` or
 * `{ "type": "mcp_server", "server_url": "...", ... }`, never a bare string.
 * A plain string is still accepted so a backend that pre-flattens the tag keeps
 * working.
 *
 * Returns `null` when no usable discriminant is present, which is the signal
 * for the caller to render nothing rather than an empty badge.
 */
export function toolKindTag(kind: unknown): string | null {
  if (typeof kind === "string") return kind.length > 0 ? kind : null;
  if (typeof kind === "object" && kind !== null) {
    const tag = (kind as { type?: unknown }).type;
    if (typeof tag === "string" && tag.length > 0) return tag;
  }
  return null;
}

/**
 * Fetches the full descriptor of a tool.
 *
 * Resolves to `null` when the tool does not exist. An empty name short-circuits
 * without a round trip, mirroring the backend contract.
 */
export async function describeTool(
  name: string,
): Promise<ToolDescriptor | null> {
  if (!name) return null;
  return invoke<ToolDescriptor | null>("describe_tool", { name });
}

/** Every registered tool, as list rows. */
export async function listTools(): Promise<ToolSummary[]> {
  return invoke<ToolSummary[]>("list_tools");
}
