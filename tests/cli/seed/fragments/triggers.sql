-- Deterministic seed fragment for the TRIGGER HISTORY subsystem.
-- Target DB   : triggers.db   (schema: schemas/triggers.sql)
-- Target tables: trigger_history, trigger_state
--
-- The desktop reads this DB on two paths:
--   1. Counters shown in the list. At boot the engine calls restore_counters ->
--      TriggerPersistence.load_counters(), which recomputes fire_count /
--      skip_count / last_fired from trigger_history (status='fired'/'skipped',
--      MAX(fired_at)). So these history rows drive the list's last-run column
--      and the fire_count read by the delete dialog.
--   2. The history sheet: automation-history-{id} -> TriggerLogs ->
--      get_trigger_logs -> GET /api/v1/triggers/:id/logs ->
--      TriggerPersistence.query_history (trigger_history ORDER BY fired_at DESC).
--
-- Seeded only for the two ACTIVE triggers (from triggers_def.sql), so their
-- history sheet is non-empty and their counters are non-zero. `fired_at` must be
-- valid RFC3339 (both read paths parse it via parse_from_rfc3339).
--
-- Note: load_counters ignores status='error' (it is neither a fire nor a skip),
-- and the list status is derived only from `enabled`, so the error row below
-- appears in the history sheet but does NOT create an "error" filter match.
-- Deterministic: fixed ids, fixed 2026-07-01 timestamps.

INSERT INTO trigger_history
  (id, trigger_id, agent_name, fired_at, task_id, status, reason, created_at, payload_json, dispatch_ms)
VALUES
  -- daily-digest: 2 fires + 1 skip -> fire_count=2, skip_count=1, last_fired set.
  ('seed-hist-digest-1', 'seed-trigger-daily-digest', 'apollia-chat',
   '2026-07-01T00:00:00Z', 'seed-task-digest-1', 'fired', NULL,
   '2026-07-01T00:00:00Z', NULL, 12),
  ('seed-hist-digest-2', 'seed-trigger-daily-digest', 'apollia-chat',
   '2026-07-01T00:00:00Z', 'seed-task-digest-2', 'fired', NULL,
   '2026-07-01T00:00:00Z', NULL, 9),
  ('seed-hist-digest-3', 'seed-trigger-daily-digest', 'apollia-chat',
   '2026-07-01T00:00:00Z', NULL, 'skipped', 'target agent busy',
   '2026-07-01T00:00:00Z', NULL, NULL),

  -- hourly-sync: 1 fire + 1 error (error exercises the history sheet only).
  ('seed-hist-sync-1', 'seed-trigger-hourly-sync', 'apollia-chat',
   '2026-07-01T00:00:00Z', 'seed-task-sync-1', 'fired', NULL,
   '2026-07-01T00:00:00Z', NULL, 15),
  ('seed-hist-sync-2', 'seed-trigger-hourly-sync', 'apollia-chat',
   '2026-07-01T00:00:00Z', NULL, 'error', 'task submission failed',
   '2026-07-01T00:00:00Z', NULL, NULL);

-- trigger_state mirrors what a real fire writes alongside the history rows
-- (persistence.rs persist_success/persist_skipped upsert this table). Kept
-- consistent with the counts above for coherence.
INSERT INTO trigger_state
  (trigger_id, last_fired, fire_count, skip_count, enabled)
VALUES
  ('seed-trigger-daily-digest', '2026-07-01T00:00:00Z', 2, 1, 1),
  ('seed-trigger-hourly-sync',  '2026-07-01T00:00:00Z', 1, 0, 1);
