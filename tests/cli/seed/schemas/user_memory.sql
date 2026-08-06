CREATE TABLE _schema_version (
        version INTEGER NOT NULL
    );
CREATE TABLE episodic_memories (
        id         TEXT PRIMARY KEY,
        namespace  TEXT NOT NULL,
        task_id    TEXT,
        agent_id   TEXT NOT NULL,
        content    TEXT NOT NULL,
        importance REAL NOT NULL DEFAULT 0.5,
        created_at TEXT NOT NULL,
        expires_at TEXT,
        metadata   TEXT NOT NULL DEFAULT '{}'
    );
CREATE TABLE semantic_memories (
        id         TEXT PRIMARY KEY,
        namespace  TEXT NOT NULL,
        key        TEXT NOT NULL,
        value      TEXT NOT NULL,
        source     TEXT,
        confidence REAL NOT NULL DEFAULT 1.0,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        expires_at TEXT,
        UNIQUE(namespace, key)
    );
CREATE TABLE procedural_memories (
        id            TEXT PRIMARY KEY,
        namespace     TEXT NOT NULL,
        trigger_text  TEXT NOT NULL,
        steps         TEXT NOT NULL,
        success_count INTEGER NOT NULL DEFAULT 1,
        last_used_at  TEXT NOT NULL,
        created_at    TEXT NOT NULL
    );
CREATE INDEX idx_episodic_namespace_created
        ON episodic_memories(namespace, created_at DESC);
CREATE INDEX idx_semantic_namespace_key
        ON semantic_memories(namespace, key);
CREATE INDEX idx_procedural_namespace_trigger
        ON procedural_memories(namespace, trigger_text);
CREATE VIRTUAL TABLE memory_fts USING fts5(content,source_table UNINDEXED,source_id UNINDEXED,tokenize='unicode61')
/* memory_fts(content,source_table,source_id) */;
CREATE TABLE IF NOT EXISTS 'memory_fts_data'(id INTEGER PRIMARY KEY, block BLOB);
CREATE TABLE IF NOT EXISTS 'memory_fts_idx'(segid, term, pgno, PRIMARY KEY(segid, term)) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS 'memory_fts_content'(id INTEGER PRIMARY KEY, c0, c1, c2);
CREATE TABLE IF NOT EXISTS 'memory_fts_docsize'(id INTEGER PRIMARY KEY, sz BLOB);
CREATE TABLE IF NOT EXISTS 'memory_fts_config'(k PRIMARY KEY, v) WITHOUT ROWID;
CREATE TABLE plan_choices (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id  TEXT NOT NULL UNIQUE,
                chosen      TEXT NOT NULL,
                plan_a_json TEXT NOT NULL,
                plan_b_json TEXT NOT NULL,
                chosen_at   INTEGER NOT NULL
            );
CREATE TABLE sqlite_sequence(name,seq);
