CREATE TABLE projects (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    description TEXT,
    instructions TEXT,
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
, workspace_path TEXT);
CREATE TABLE project_documents (
    id          TEXT PRIMARY KEY,
    project_id  TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    name        TEXT NOT NULL,
    file_path   TEXT NOT NULL,
    size_bytes  INTEGER NOT NULL DEFAULT 0,
    uploaded_at TEXT NOT NULL
);
CREATE TABLE project_providers (
    id            TEXT PRIMARY KEY,
    project_id    TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    provider_type TEXT NOT NULL,
    name          TEXT NOT NULL,
    config_json   TEXT NOT NULL DEFAULT '{}',
    path          TEXT,
    enabled       INTEGER NOT NULL DEFAULT 1,
    priority      INTEGER NOT NULL DEFAULT 50
);
CREATE TABLE project_templates (
    id                   TEXT PRIMARY KEY,
    name                 TEXT NOT NULL,
    description          TEXT,
    providers_config_json TEXT NOT NULL DEFAULT '[]',
    is_builtin           INTEGER NOT NULL DEFAULT 0,
    created_at           TEXT NOT NULL
);
CREATE INDEX idx_project_documents_project ON project_documents(project_id);
CREATE INDEX idx_project_providers_project ON project_providers(project_id);
CREATE TABLE project_agents (
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    agent_name TEXT NOT NULL,
    added_at   TEXT NOT NULL,
    PRIMARY KEY (project_id, agent_name)
);
CREATE INDEX idx_project_agents_agent ON project_agents(agent_name);
