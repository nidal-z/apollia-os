// ─── Projects and workspace ───

// ─── Projects ────────────────────────────────────────────────────────────────

/** Summary of one project in the list. */
export interface ProjectSummary {
  id: string;
  name: string;
  description: string | null;
  created_at: string;
  updated_at: string;
  workspace_path: string | null;
}

/** Document attached to a project. */
export interface ProjectDocument {
  id: string;
  project_id: string;
  name: string;
  file_path: string;
  size_bytes: number;
  uploaded_at: string;
}

/** Context provider configured for a project. */
export interface ProjectProviderRow {
  id: string;
  project_id: string;
  provider_type: string;
  name: string;
  config_json: string;
  path: string | null;
  enabled: boolean;
  priority: number;
}

/** Full detail of one project. */
export interface ProjectDetail {
  id: string;
  name: string;
  description: string | null;
  instructions: string | null;
  created_at: string;
  updated_at: string;
  workspace_path: string | null;
  documents: ProjectDocument[];
  providers: ProjectProviderRow[];
  agents: string[];
}

/** Predefined project template. */
export interface ProjectTemplate {
  id: string;
  name: string;
  description: string | null;
  providers_config_json: string;
  is_builtin: boolean;
  created_at: string;
}

/** Payload creating a project. */
export interface CreateProjectRequest {
  name: string;
  description?: string;
  instructions?: string;
  workspace_path?: string;
}

/** Payload partially updating a project. */
export interface UpdateProjectRequest {
  name?: string;
  description?: string | null;
  instructions?: string | null;
  workspace_path?: string | null;
}

/** Section of a workspace snapshot. */
export interface WorkspaceSectionView {
  source: string;
  title: string;
  content: string;
}

/** Live workspace snapshot of a project. */
export interface WorkspaceSnapshotView {
  sections: WorkspaceSectionView[];
  error_count: number;
}

/** CLI install status (Docker Desktop model). */
export interface CliStatus {
  bundled: boolean;
  bundled_path: string | null;
  installed: boolean;
  symlink_path: string;
  version: string;
  needs_privilege: boolean;
}
