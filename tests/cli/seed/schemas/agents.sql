CREATE TABLE installed_agents (
    name            TEXT PRIMARY KEY,
    version         TEXT NOT NULL,
    install_path    TEXT NOT NULL,       -- ~/.apollia/agents/<name>/agent.py
    source_path     TEXT NOT NULL,       -- original path of the installed file
    manifest_json   TEXT NOT NULL,       -- AgentManifest serialised as JSON
    enabled         BOOLEAN NOT NULL DEFAULT 1,
    installed_at    TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at      TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX idx_installed_agents_enabled
    ON installed_agents(enabled);
CREATE TABLE installed_packages (
    name          TEXT PRIMARY KEY,
    version       TEXT NOT NULL,
    -- Absolute path of the installed package directory (~/.apollia/agents/packages/<name>/)
    root_path     TEXT NOT NULL,
    -- PackageManifest serialised as JSON (includes declared tools, triggers, pip)
    manifest_json TEXT NOT NULL,
    installed_at  TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at    TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE TABLE package_agents (
    package_name TEXT NOT NULL REFERENCES installed_packages(name) ON DELETE CASCADE,
    -- agent_name is a soft reference to installed_agents(name); cascade handled at app layer.
    agent_name   TEXT NOT NULL,
    PRIMARY KEY (package_name, agent_name)
);
CREATE INDEX idx_package_agents_package ON package_agents(package_name);
CREATE INDEX idx_package_agents_agent   ON package_agents(agent_name);
