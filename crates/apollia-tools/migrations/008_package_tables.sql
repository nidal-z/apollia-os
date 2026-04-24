-- Migration 008 : Agent Package System
--
-- Introduit deux tables pour lier les packages d'agents (agent.toml) aux agents
-- individuels déjà enregistrés dans installed_agents.
--
-- installed_packages : source de vérité des packages installés.
-- package_agents     : lien package ↔ agent (ON DELETE CASCADE).
--
-- Les agents restent dans installed_agents — source de vérité unique pour Phase 11.

CREATE TABLE IF NOT EXISTS installed_packages (
    name          TEXT PRIMARY KEY,
    version       TEXT NOT NULL,
    -- Chemin absolu du dossier package installé (~/.apollia/agents/packages/<name>/)
    root_path     TEXT NOT NULL,
    -- PackageManifest sérialisé en JSON (inclut tools, triggers déclarés, pip)
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
