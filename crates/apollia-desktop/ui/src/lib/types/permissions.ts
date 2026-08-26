// ─── HITL permission requests ───

// ─── HITL Permission Types ────────────────────────────────────────────────────

/** Fields common to every HITL permission request. */
export interface BasePermission {
  /** Discriminant of the permission type. */
  permission_type: string;
  /** Unique identifier of the permission request. */
  request_id: string;
  /** Identifier of the agent behind the request. */
  agent_id: string;
}

/** Permission request to run a bash command. */
export interface BashPermission extends BasePermission {
  permission_type: 'bash';
  /** Full command to run. */
  command: string;
  /** Working directory of the command. */
  working_dir: string;
}

/** Permission request to edit an existing file. */
export interface FileEditPermission extends BasePermission {
  permission_type: 'file_edit';
  /** Path of the modified file. */
  file_path: string;
  /** Original content (before the edit). */
  old_content: string;
  /** Resulting content (after the edit). */
  new_content: string;
}

/** Permission request to write a file (creation or overwrite). */
export interface FileWritePermission extends BasePermission {
  permission_type: 'file_write';
  /** Path of the target file. */
  file_path: string;
  /** Content to write. */
  content: string;
  /** Mode: creating a new file, or overwriting an existing one. */
  mode: 'create' | 'overwrite';
}

/** Permission request for a filesystem operation. */
export interface FilesystemPermission extends BasePermission {
  permission_type: 'filesystem';
  /** Type of operation. */
  operation: 'delete' | 'move' | 'mkdir';
  /** Source path of the operation. */
  path: string;
  /** Destination path (only for `move`). */
  target_path?: string;
}

/** Permission request to invoke an MCP tool. */
export interface McpPermission extends BasePermission {
  permission_type: 'mcp';
  /** Name of the MCP server exposing the tool. */
  server_name: string;
  /** Name of the invoked MCP tool. */
  tool_name: string;
  /** Arguments passed to the tool. */
  arguments: Record<string, unknown>;
}

/** Generic permission request (untyped tool). */
export interface GenericPermission extends BasePermission {
  permission_type: 'generic';
  /** Tool name. */
  tool_name: string;
  /** Raw tool input. */
  input: Record<string, unknown>;
}

/** Discriminated union of every HITL permission request. */
export type ApollaPermission =
  | BashPermission
  | FileEditPermission
  | FileWritePermission
  | FilesystemPermission
  | McpPermission
  | GenericPermission;

/** MCP server summary returned by `list_mcp_servers` and `restart_mcp_server`. */
