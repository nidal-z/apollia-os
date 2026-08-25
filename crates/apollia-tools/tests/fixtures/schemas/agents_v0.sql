-- agents.db as a pre-versioning binary wrote it (user_version = 0): the
-- 007_agent_tables and 008_package_tables shapes, written by a binary
-- that did not stamp a version. Frozen fixture: do not update it when
-- the live schema evolves, it is the "old format" the migration tests open.
CREATE TABLE IF NOT EXISTS installed_agents (
    name            TEXT PRIMARY KEY,
    version         TEXT NOT NULL,
    install_path    TEXT NOT NULL,
    source_path     TEXT NOT NULL,
    manifest_json   TEXT NOT NULL,
    enabled         BOOLEAN NOT NULL DEFAULT 1,
    installed_at    TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at      TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX IF NOT EXISTS idx_installed_agents_enabled
    ON installed_agents(enabled);

CREATE TABLE IF NOT EXISTS installed_packages (
    name          TEXT PRIMARY KEY,
    version       TEXT NOT NULL,
    root_path     TEXT NOT NULL,
    manifest_json TEXT NOT NULL,
    installed_at  TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at    TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS package_agents (
    package_name TEXT NOT NULL REFERENCES installed_packages(name) ON DELETE CASCADE,
    agent_name   TEXT NOT NULL,
    PRIMARY KEY (package_name, agent_name)
);

CREATE INDEX IF NOT EXISTS idx_package_agents_package ON package_agents(package_name);
CREATE INDEX IF NOT EXISTS idx_package_agents_agent   ON package_agents(agent_name);
