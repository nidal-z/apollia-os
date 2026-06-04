-- Migration 006 - table runtime_events (event-sourced observability)
-- Idempotente : CREATE TABLE / TRIGGER / INDEX IF NOT EXISTS.
--
-- Append-only event log qui devient la source de vérité de la trajectoire
-- d'exécution d'un agent. Couvre : ctx.log, thoughts ReAct, tool calls,
-- LLM calls, retries, A2A invocations, transitions HITL, etc.
--
-- Event-sourced observability via `runtime_events`.

CREATE TABLE IF NOT EXISTS runtime_events (
    -- UUID v7 (ordonnable lexicographiquement par création).
    event_id         TEXT PRIMARY KEY,

    -- Tâche dont fait partie cet événement.
    task_id          TEXT NOT NULL,

    -- Agent qui a émis l'événement.
    agent_id         TEXT NOT NULL,

    -- Self-FK : nesting (tool_call_completed → tool_call_started ; A2A child → invoke).
    parent_event_id  TEXT,

    -- ID partagé sur une chaîne A2A complète. Permet de récupérer toute la
    -- trajectoire d'une délégation par WHERE correlation_id = ?.
    correlation_id   TEXT,

    -- Numéro du tour ReAct (NULL hors loop).
    step_num         INTEGER,

    -- Discriminant typé. Voir EventKind dans apollia-core/src/events.rs.
    -- Valeurs : task_submitted, task_state_changed, react_turn_started,
    --          thought, llm_call_started, llm_call_completed, llm_call_failed,
    --          action_parse_error, tool_call_started, tool_call_completed,
    --          tool_call_denied, a2a_invoke_started, a2a_invoke_completed,
    --          agent_log, hitl_suspended, hitl_resumed, retry,
    --          task_completed, task_failed, bus_lagged.
    kind             TEXT NOT NULL,

    -- Payload typé par kind, sérialisé en JSON. Schema versionné par
    -- l'enum EventKind côté Rust.
    payload_json     TEXT NOT NULL,

    -- ISO 8601 avec millisecondes (RFC 3339).
    ts               TEXT NOT NULL,

    -- Timestamp Unix en secondes pour purge efficace par rétention.
    created_at_unix  INTEGER NOT NULL
);

-- Recherche par tâche, ordonnée chronologiquement (cas d'usage principal).
CREATE INDEX IF NOT EXISTS idx_runtime_events_task_ts
    ON runtime_events(task_id, ts);

-- Reconstruction d'un sous-arbre (tool_call → completion ; A2A invoke → child).
CREATE INDEX IF NOT EXISTS idx_runtime_events_parent
    ON runtime_events(parent_event_id);

-- Reconstruction d'une chaîne A2A complète.
CREATE INDEX IF NOT EXISTS idx_runtime_events_correlation
    ON runtime_events(correlation_id);

-- Purge par rétention (DELETE WHERE created_at_unix < threshold).
CREATE INDEX IF NOT EXISTS idx_runtime_events_created_at
    ON runtime_events(created_at_unix);

-- Garantie d'immuabilité : append-only. Toute UPDATE/DELETE depuis
-- l'application est refusée. La purge par rétention contourne via
-- `PRAGMA defer_foreign_keys = ON ; DELETE FROM runtime_events WHERE …`
-- exécuté via la routine de maintenance dédiée (qui DROP+CREATE le trigger
-- temporairement).
CREATE TRIGGER IF NOT EXISTS runtime_events_no_update
BEFORE UPDATE ON runtime_events
BEGIN
    SELECT RAISE(ABORT, 'runtime_events is append-only');
END;
