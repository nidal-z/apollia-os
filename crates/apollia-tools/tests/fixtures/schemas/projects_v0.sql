-- projects.db as a pre-versioning binary wrote it (user_version = 0):
-- the 010_projects and 009_project_agents shapes, before the additive
-- workspace_path column. Frozen fixture: do not update it when the live
-- schema evolves, it is the "old format" the migration tests open.
-- Migration 008 - projects
-- Idempotente : CREATE TABLE IF NOT EXISTS / CREATE INDEX IF NOT EXISTS

CREATE TABLE IF NOT EXISTS projects (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    description TEXT,
    instructions TEXT,
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS project_documents (
    id          TEXT PRIMARY KEY,
    project_id  TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    name        TEXT NOT NULL,
    file_path   TEXT NOT NULL,
    size_bytes  INTEGER NOT NULL DEFAULT 0,
    uploaded_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS project_providers (
    id            TEXT PRIMARY KEY,
    project_id    TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    provider_type TEXT NOT NULL,
    name          TEXT NOT NULL,
    config_json   TEXT NOT NULL DEFAULT '{}',
    path          TEXT,
    enabled       INTEGER NOT NULL DEFAULT 1,
    priority      INTEGER NOT NULL DEFAULT 50
);

CREATE TABLE IF NOT EXISTS project_templates (
    id                   TEXT PRIMARY KEY,
    name                 TEXT NOT NULL,
    description          TEXT,
    providers_config_json TEXT NOT NULL DEFAULT '[]',
    is_builtin           INTEGER NOT NULL DEFAULT 0,
    created_at           TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_project_documents_project ON project_documents(project_id);
CREATE INDEX IF NOT EXISTS idx_project_providers_project ON project_providers(project_id);

-- Migration 009 - project_agents junction table.
-- Links agents to projects (many-to-many). agent_name references installed_agents
-- in agents.db (application-level, no FK - separate databases).

CREATE TABLE IF NOT EXISTS project_agents (
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    agent_name TEXT NOT NULL,
    added_at   TEXT NOT NULL,
    PRIMARY KEY (project_id, agent_name)
);

CREATE INDEX IF NOT EXISTS idx_project_agents_agent ON project_agents(agent_name);
