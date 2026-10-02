-- agents.db v2: an agent is a definition plus the user's overrides, and a
-- package says what it installed.
--
-- agents          : one row per agent. `definition_json` is the agent as its
--                   package declares it (the `[[agents]]` entry of
--                   apollia.toml, as JSON), never written by the user; an
--                   update replaces it. The code columns (install_path,
--                   source_path, manifest_json) are set for an agent that runs
--                   Python code and left NULL for a declarative agent.
-- agent_overrides : the user's local values, one row per overridden field. An
--                   update leaves them in place; "back to defaults" deletes
--                   them.
-- package_items   : what a package installed, by kind.
--
-- The rows of installed_agents and package_agents are copied by the Rust
-- step of the migration (crate::agents_db::migrate_v2), which then drops the
-- two tables.

CREATE TABLE IF NOT EXISTS agents (
    name            TEXT PRIMARY KEY,
    origin          TEXT NOT NULL DEFAULT 'package'
                    CHECK (origin IN ('package', 'interface')),
    package         TEXT,
    version         TEXT NOT NULL,
    definition_json TEXT NOT NULL,
    install_path    TEXT,
    source_path     TEXT,
    manifest_json   TEXT,
    enabled         BOOLEAN NOT NULL DEFAULT 1,
    installed_at    TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at      TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_agents_enabled ON agents(enabled);

CREATE TABLE IF NOT EXISTS agent_overrides (
    agent       TEXT NOT NULL,
    field       TEXT NOT NULL,
    value_json  TEXT NOT NULL,
    updated_at  TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (agent, field)
);

CREATE TABLE IF NOT EXISTS package_items (
    package_name TEXT NOT NULL REFERENCES installed_packages(name) ON DELETE CASCADE,
    kind         TEXT NOT NULL CHECK (kind IN ('agent', 'workflow', 'tool', 'connection')),
    item_id      TEXT NOT NULL,
    PRIMARY KEY (package_name, kind, item_id)
);

CREATE INDEX IF NOT EXISTS idx_package_items_item ON package_items(kind, item_id);
