-- Opt-in seed fragment for triggers_def.db: the planner's trigger.
-- Applied by build-seed.sh only when APOLLIA_SEED_PLANNER=1, after the base
-- fragment. The base fixture stays at four triggers.
--
-- The desktop fires it from automation-run-now-seed-trigger-planner
-- (ui/src/components/automations/AutomationRow.svelte, fireNow -> fire_trigger
-- -> POST /api/v1/triggers/:id/fire -> TriggerEngine::fire_now), which needs
-- no picker and no confirm, and is enabled only while `enabled` is 1.
--
-- The cron schedule is one instant a year (1 January, 03:00), so the trigger
-- never fires on its own during a run. `{{fired_at}}` is rendered from the
-- Timer payload a manual fire builds (crates/apollia-triggers/src/engine/
-- commands.rs cmd_fire_now), so two fires never share a task text: the ORIA
-- plan cache keys on the task text and a cache hit executes the cached plan
-- WITHOUT opening the gate (crates/apollia-oria/src/engine/plan_cache_ops.rs
-- execute_cached_plan).

INSERT INTO trigger_definitions
  (id, agent, enabled, on_busy, source_type, source_config, input_template, created_at, updated_at)
VALUES
  ('seed-trigger-planner',
   'seed-planner',
   1,
   'queue',
   'cron',
   '{"schedule":"0 0 3 1 1 * *"}',
   'Draft the agenda of the weekly team meeting in three reasoning steps. Fired at {{fired_at}}.',
   '2026-07-01T00:00:00Z',
   '2026-07-01T00:00:00Z');
