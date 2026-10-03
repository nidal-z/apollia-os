-- agents.db v5: the agents and workflows the user exposes to an MCP client.
--
-- exposed_items : one row per item served by the MCP bridge
--                 (`apollia-os mcp server`). `kind` is `agent` or `workflow`;
--                 `item_id` is the agent name, or `<package>/<workflow>`. An
--                 item without a row is not exposed, which is the default.

CREATE TABLE IF NOT EXISTS exposed_items (
    kind       TEXT NOT NULL CHECK (kind IN ('agent', 'workflow')),
    item_id    TEXT NOT NULL,
    exposed_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (kind, item_id)
);
