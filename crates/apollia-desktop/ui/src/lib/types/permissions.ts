// ─── HITL permission requests ───

// ─── HITL Permission Types ────────────────────────────────────────────────────

/** Champs communs à toutes les demandes de permission HITL. */
export interface BasePermission {
  /** Discriminant du type de permission. */
  permission_type: string;
  /** Identifiant unique de la demande de permission. */
  request_id: string;
  /** Identifiant de l'agent à l'origine de la demande. */
  agent_id: string;
}

/** Demande de permission pour l'exécution d'une commande bash. */
export interface BashPermission extends BasePermission {
  permission_type: 'bash';
  /** Commande complète à exécuter. */
  command: string;
  /** Répertoire de travail de la commande. */
  working_dir: string;
}

/** Demande de permission pour l'édition d'un fichier existant. */
export interface FileEditPermission extends BasePermission {
  permission_type: 'file_edit';
  /** Chemin du fichier modifié. */
  file_path: string;
  /** Contenu original (avant modification). */
  old_content: string;
  /** Contenu résultant (après modification). */
  new_content: string;
}

/** Demande de permission pour l'écriture d'un fichier (création ou écrasement). */
export interface FileWritePermission extends BasePermission {
  permission_type: 'file_write';
  /** Chemin du fichier ciblé. */
  file_path: string;
  /** Contenu à écrire. */
  content: string;
  /** Mode : création d'un nouveau fichier ou écrasement d'un existant. */
  mode: 'create' | 'overwrite';
}

/** Demande de permission pour une opération sur le système de fichiers. */
export interface FilesystemPermission extends BasePermission {
  permission_type: 'filesystem';
  /** Type d'opération. */
  operation: 'delete' | 'move' | 'mkdir';
  /** Chemin source de l'opération. */
  path: string;
  /** Chemin destination (uniquement pour `move`). */
  target_path?: string;
}

/** Demande de permission pour l'invocation d'un outil MCP. */
export interface McpPermission extends BasePermission {
  permission_type: 'mcp';
  /** Nom du serveur MCP exposant l'outil. */
  server_name: string;
  /** Nom de l'outil MCP invoqué. */
  tool_name: string;
  /** Arguments transmis à l'outil. */
  arguments: Record<string, unknown>;
}

/** Demande de permission générique (outil non typé). */
export interface GenericPermission extends BasePermission {
  permission_type: 'generic';
  /** Nom de l'outil. */
  tool_name: string;
  /** Entrée brute de l'outil. */
  input: Record<string, unknown>;
}

/** Union discriminée de toutes les demandes de permission HITL. */
export type ApollaPermission =
  | BashPermission
  | FileEditPermission
  | FileWritePermission
  | FilesystemPermission
  | McpPermission
  | GenericPermission;

/** MCP server summary returned by `list_mcp_servers` and `restart_mcp_server`. */
