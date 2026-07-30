-- Deterministic seed fragment for the AUTOMATIONS / TRIGGERS subsystem.
-- Target DB  : triggers_def.db   (schema: schemas/triggers_def.sql)
-- Target table: trigger_definitions
--
-- This is the table the desktop Automations page actually lists. Read path:
--   Automations.svelte -> $triggers store -> invoke("list_triggers")
--   -> IPC list_triggers (commands/triggers.rs) -> GET /api/v1/triggers
--   -> routes_triggers::list_triggers -> TriggerEngine.list()
-- The engine is loaded at Supervisor boot from THIS table
--   (boot.rs: TriggerDefinitionRepository::open(data_dir/"triggers_def.db")
--    -> load_trigger_definitions -> engine.reload). build_statuses() lists
-- every definition (enabled and disabled), so both active and paused rows show.
--
-- Unblocks (AutomationRow keys every testid off trigger.id):
--   * automation-row-{id}, automation-run-now-{id}, automation-menu-{id}
--   * automation-enabled-toggle-{id}, automation-history-{id},
--     automation-delete-{id} -> delete-automation-dialog / -skip / -confirm
--   * filter chips automations-filter-{all,active,paused}
--
-- Filter coverage: the UI derives status purely from `enabled`
-- (Automations.svelte statusOf: enabled -> "active", else "paused"). There is
-- NO error status in the read model (no status/last-error column here, and
-- statusOf never returns "error"), so the "error" chip cannot be seeded from
-- data. See the run report for this residual gap.
--
-- source_config is a JSON string whose keys must match parse_source_config
-- (definition_repository.rs): cron->schedule, interval->every,
-- file_watch->path/events, webhook->secret. Agent target `apollia-chat` is a
-- real seeded agent. Deterministic: fixed ids, fixed 2026-07-01 timestamps.

INSERT INTO trigger_definitions
  (id, agent, enabled, on_busy, source_type, source_config, input_template, created_at, updated_at)
VALUES
  -- ACTIVE (enabled=1) - cron. Has fire history seeded in triggers.db, so the
  -- list shows a non-zero last-run and fire_count.
  ('seed-trigger-daily-digest',
   'apollia-chat',
   1,
   'queue',
   'cron',
   '{"schedule":"0 0 8 * * MON *"}',
   'Prepare the weekly digest for {{scheduled_at}}.',
   '2026-07-01T00:00:00Z',
   '2026-07-01T00:00:00Z'),

  -- ACTIVE (enabled=1) - interval. Second active row and second source type.
  ('seed-trigger-hourly-sync',
   'apollia-chat',
   1,
   'queue',
   'interval',
   '{"every":"1h"}',
   'Run the hourly synchronization pass.',
   '2026-07-01T00:00:00Z',
   '2026-07-01T00:00:00Z'),

  -- PAUSED (enabled=0) - file_watch. Proves the "paused" chip has a match and
  -- adds a third source type.
  ('seed-trigger-inbox-watch',
   'apollia-chat',
   0,
   'drop',
   'file_watch',
   '{"path":"__APOLLIA_SEED_HOME__/Downloads","events":["create","modify"],"recursive":false}',
   'A new file landed in the watched folder: {{path}}.',
   '2026-07-01T00:00:00Z',
   '2026-07-01T00:00:00Z'),

  -- PAUSED (enabled=0) - webhook. Fourth source type; secret is >= 32 chars for
  -- realism (SQL insert bypasses repo validation, but the engine reads it as-is).
  ('seed-trigger-webhook-deploy',
   'apollia-chat',
   0,
   'queue',
   'webhook',
   '{"secret":"seed-webhook-secret-0123456789abcdef"}',
   'Deploy hook received: {{payload}}.',
   '2026-07-01T00:00:00Z',
   '2026-07-01T00:00:00Z');
