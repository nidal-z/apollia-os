-- Migration 007 — installed_agents
-- Idempotente : CREATE TABLE IF NOT EXISTS / CREATE INDEX IF NOT EXISTS

CREATE TABLE IF NOT EXISTS installed_agents (
    name            TEXT PRIMARY KEY,
    version         TEXT NOT NULL,
    install_path    TEXT NOT NULL,       -- ~/.apollia/agents/<name>/agent.py
    source_path     TEXT NOT NULL,       -- chemin original du fichier installé
    manifest_json   TEXT NOT NULL,       -- AgentManifest sérialisé en JSON
    enabled         BOOLEAN NOT NULL DEFAULT 1,
    installed_at    TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at      TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_installed_agents_enabled
    ON installed_agents(enabled);
