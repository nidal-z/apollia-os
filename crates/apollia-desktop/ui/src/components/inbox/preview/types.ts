/**
 * Shared types for permission preview payloads.
 *
 * Each preview kind has its own shape; a dispatcher picks the right
 * renderer based on `kind`. Payloads are produced by the backend
 * `preview_*_action` commands.
 */

export type PreviewKind = "filesystem" | "bash" | "mcp" | "network" | "generic";

export interface FilesystemChange {
  /** Absolute or workspace-relative path. */
  path: string;
  action: "create" | "update" | "delete" | "chmod";
  /** Unified diff lines (rendered red/green). Optional for chmod/delete. */
  diff?: string;
  /** Total lines changed; used to gate the "show more" behavior. */
  total_lines?: number;
  /** For chmod. */
  old_mode?: string;
  new_mode?: string;
}

export interface FilesystemPreview {
  kind: "filesystem";
  changes: FilesystemChange[];
}

export interface BashPreview {
  kind: "bash";
  /** The command as it will be executed. */
  command: string;
  /** `true` if `--dry-run` was supported and output follows. */
  dry_run_supported: boolean;
  /** Output from the dry-run, if any. */
  dry_run_output?: string;
  /** Statically detected commands in a pipeline. */
  detected_commands?: string[];
  /** shellcheck warnings keyed by severity. */
  warnings?: { severity: "info" | "warning" | "error"; message: string }[];
}

export interface McpParameter {
  name: string;
  type: string;
  required: boolean;
  description?: string;
  example?: unknown;
}

export interface McpPreview {
  kind: "mcp";
  tool_name: string;
  server_name?: string;
  input_schema_parameters: McpParameter[];
  output_schema_summary?: string;
  example_call?: Record<string, unknown>;
}

export interface NetworkPreview {
  kind: "network";
  method: string;
  url: string;
  headers?: { name: string; value: string }[];
  estimated_body_bytes?: number;
  body_excerpt?: string;
}

export interface GenericPreview {
  kind: "generic";
  description: string;
  raw?: unknown;
}

export type PermissionPreviewPayload =
  | FilesystemPreview
  | BashPreview
  | McpPreview
  | NetworkPreview
  | GenericPreview;

export interface PermissionImpactPayload {
  resource?: string;
  action?: string;
  magnitude?: string;
  summary?: string;
  consequence_if_approved?: string;
  consequence_if_refused?: string;
  risk_level?: "low" | "medium" | "high" | "critical";
  risk_reasons?: string[];
}
