-- Migration 008 : Agent Package System
--
-- Introduces two tables linking the agent packages (agent.toml) to the
-- individual agents already registered in installed_agents.
--
-- installed_packages: the source of truth for the installed packages.
-- package_agents     : lien package ↔ agent (ON DELETE CASCADE).
--
-- The agents stay in installed_agents, the single source of truth.

CREATE TABLE IF NOT EXISTS installed_packages (
    name          TEXT PRIMARY KEY,
    version       TEXT NOT NULL,
    -- Absolute path of the installed package directory (~/.apollia/agents/packages/<name>/)
    root_path     TEXT NOT NULL,
    -- PackageManifest serialised as JSON (includes declared tools, triggers, pip)
    manifest_json TEXT NOT NULL,
    installed_at  TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at    TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS package_agents (
    package_name TEXT NOT NULL REFERENCES installed_packages(name) ON DELETE CASCADE,
    -- agent_name is a soft reference to installed_agents(name); cascade handled at app layer.
    agent_name   TEXT NOT NULL,
    PRIMARY KEY (package_name, agent_name)
);

CREATE INDEX IF NOT EXISTS idx_package_agents_package ON package_agents(package_name);
CREATE INDEX IF NOT EXISTS idx_package_agents_agent   ON package_agents(agent_name);
