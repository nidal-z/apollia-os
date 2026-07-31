-- Deterministic seed fragment for the PROJECTS subsystem (projects.db).
-- Tables: projects, project_providers, project_documents.
-- Unblocks the desktop Projects page automation (projects-det.json):
--   * project-row-*        (list, Projects.svelte)  <- projects
--   * settings-name-input / settings-description-input /
--     settings-instructions-input / settings-workspace-input
--                           (Settings tab, reads the selected project)
--   * providers-list, provider-card-*, provider-toggle-*, provider-delete-*,
--     provider-edit-*      (Context tab, ProviderCard.svelte) <- project_providers
--   * project documents list (ProjectDocument) <- project_documents
-- Schema is applied separately by the builder (schemas/projects.sql).
-- Deterministic: fixed ids, fixed timestamps, no random values.

-- Projects. Ordered alphabetically by name in the UI (list_projects ORDER BY
-- name ASC), so "Seed Project Alpha" is row nth 0 and owns the providers.
INSERT INTO projects (id, name, description, instructions, workspace_path, created_at, updated_at) VALUES
  ('seed-project-alpha',
   'Seed Project Alpha',
   'Deterministic seed project used by the projects-det automation run.',
   'Follow the repository conventions. This instructions field is seeded so the settings tab renders a non-empty value.',
   '__APOLLIA_SEED_WORKSPACE__',
   '2026-07-01T00:00:00Z',
   '2026-07-01T00:00:00Z'),
  ('seed-project-beta',
   'Seed Project Beta',
   'Second deterministic seed project to prove the list renders more than one row.',
   'Secondary project instructions, seeded for settings-tab parity.',
   '__APOLLIA_SEED_WORKSPACE__',
   '2026-07-01T00:00:00Z',
   '2026-07-01T00:00:00Z');

-- Context providers for the first project (Alpha). provider_type values match
-- the built-in "Developer project" template (git, rules, tree). Ordered by
-- priority ASC in the UI. config_json must be valid JSON (schema default '{}').
INSERT INTO project_providers (id, project_id, provider_type, name, config_json, path, enabled, priority) VALUES
  ('seed-provider-alpha-git',
   'seed-project-alpha',
   'git',
   'Git Status',
   '{}',
   '__APOLLIA_SEED_WORKSPACE__',
   1,
   10),
  ('seed-provider-alpha-rules',
   'seed-project-alpha',
   'rules',
   'Project Rules',
   '{}',
   '__APOLLIA_SEED_WORKSPACE__',
   1,
   20),
  ('seed-provider-alpha-tree',
   'seed-project-alpha',
   'tree',
   'Directory Tree',
   '{}',
   '__APOLLIA_SEED_WORKSPACE__',
   1,
   30);

-- One document attached to the first project (Alpha). Listed by get_project
-- via project_documents ORDER BY uploaded_at ASC.
INSERT INTO project_documents (id, project_id, name, file_path, size_bytes, uploaded_at) VALUES
  ('seed-document-alpha-readme',
   'seed-project-alpha',
   'README.md',
   '__APOLLIA_SEED_WORKSPACE__/README.md',
   2048,
   '2026-07-01T00:00:00Z');
