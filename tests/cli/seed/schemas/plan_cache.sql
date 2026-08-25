CREATE TABLE plan_cache (
    cache_key     TEXT PRIMARY KEY,
    plan_json     TEXT NOT NULL,
    hit_count     INTEGER NOT NULL DEFAULT 0,
    created_at    TEXT NOT NULL,
    last_used_at  TEXT NOT NULL,
    agent_name    TEXT NOT NULL,
    agent_version TEXT NOT NULL
);
