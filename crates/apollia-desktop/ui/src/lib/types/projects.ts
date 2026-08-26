// ─── Projects and workspace ───

// ─── Projects ────────────────────────────────────────────────────────────────

/** Résumé d'un projet dans la liste. */
export interface ProjectSummary {
  id: string;
  name: string;
  description: string | null;
  created_at: string;
  updated_at: string;
  workspace_path: string | null;
}

/** Document attaché à un projet. */
export interface ProjectDocument {
  id: string;
  project_id: string;
  name: string;
  file_path: string;
  size_bytes: number;
  uploaded_at: string;
}

/** Provider de contexte configuré pour un projet. */
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

/** Détail complet d'un projet. */
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

/** Template de projet prédéfini. */
export interface ProjectTemplate {
  id: string;
  name: string;
  description: string | null;
  providers_config_json: string;
  is_builtin: boolean;
  created_at: string;
}

/** Payload pour la création d'un projet. */
export interface CreateProjectRequest {
  name: string;
  description?: string;
  instructions?: string;
  workspace_path?: string;
}

/** Payload pour la mise à jour partielle d'un projet. */
export interface UpdateProjectRequest {
  name?: string;
  description?: string | null;
  instructions?: string | null;
  workspace_path?: string | null;
}

/** Section d'un snapshot workspace. */
export interface WorkspaceSectionView {
  source: string;
  title: string;
  content: string;
}

/** Snapshot workspace live d'un projet. */
export interface WorkspaceSnapshotView {
  sections: WorkspaceSectionView[];
  error_count: number;
}

/** Status de l'installation CLI (modèle Docker Desktop). */
export interface CliStatus {
  bundled: boolean;
  bundled_path: string | null;
  installed: boolean;
  symlink_path: string;
  version: string;
  needs_privilege: boolean;
}
