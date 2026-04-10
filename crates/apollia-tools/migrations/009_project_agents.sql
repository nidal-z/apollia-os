-- Migration 009 — project_agents junction table.
-- Links agents to projects (many-to-many). agent_name references installed_agents
-- in agents.db (application-level, no FK — separate databases).

CREATE TABLE IF NOT EXISTS project_agents (
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    agent_name TEXT NOT NULL,
    added_at   TEXT NOT NULL,
    PRIMARY KEY (project_id, agent_name)
);

CREATE INDEX IF NOT EXISTS idx_project_agents_agent ON project_agents(agent_name);
