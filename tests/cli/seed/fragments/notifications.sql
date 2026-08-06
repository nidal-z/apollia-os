-- Deterministic seed fragment for the NOTIFICATIONS subsystem (notifications.db).
-- Tables: notification_channels, notification_global_events, notification_logs.
-- Schema is applied separately by the builder (schemas/notifications.sql).
--
-- Unblocks the desktop Notifications + Inbox automation:
--   * channel-card-* / channel-toggle-* / channel-edit-btn-* / channel-test-btn-* /
--     channel-delete-btn-*  (Notifications.svelte, list_notification_channels)
--                           <- notification_channels
--   * global-events-section + the global-event-* toggles reflect enabled state
--                           (GlobalEventsEditor, get_notification_events)
--                           <- notification_global_events
--   * inbox-tab-notifications log list (get_notification_logs)
--   * inbox-tab-activity activity-row-* (list_runtime_activity, filtered to
--     task.failed / agent.degraded / trigger.error / llm.backend_down)
--                           <- notification_logs
--
-- The notifications-det.json script creates its OWN channel `det-auto-channel`
-- in-script. The seeded channel ids below are deliberately distinct
-- (seed-channel-desktop, seed-channel-webhook) so they never collide with it.
--
-- Deterministic: fixed ids, fixed timestamps, no random values. The desktop
-- notification engine also bootstraps a default channel on first launch; its id
-- does not collide with the seed ids here.

-- Channels. channel_type is constrained to ('desktop','webhook'). config_json
-- must be valid JSON. events_json NULL means "use the global events"; a JSON
-- array means channel-specific events. Listed in the UI grid as
-- channel-card-<id>.
INSERT INTO notification_channels
  (id, channel_type, enabled, config_json, events_json, label, min_interval_seconds, created_at, updated_at)
VALUES
  ('seed-channel-desktop',
   'desktop',
   1,
   '{}',
   NULL,
   'Seed Desktop',
   0,
   '2026-07-01T00:00:00Z',
   '2026-07-01T00:00:00Z'),
  ('seed-channel-webhook',
   'webhook',
   0,
   '{"url":"https://example.invalid/seed-hook"}',
   '["task.completed","task.failed"]',
   'Seed Webhook',
   300,
   '2026-07-01T00:00:00Z',
   '2026-07-01T00:00:00Z');

-- Global events. A row means the event is enabled. The GlobalEventsEditor
-- renders every known event type as a toggle; these three come back enabled
-- from get_notification_events, the remaining known types render disabled.
INSERT INTO notification_global_events (event_name) VALUES
  ('task.completed'),
  ('task.failed'),
  ('agent.degraded');

-- Notification history. Covers the Inbox "notifications" tab (all events) and
-- the Inbox "activity" tab (task.failed / agent.degraded / trigger.error /
-- llm.backend_down only). channels is a JSON map of per-channel outcome.
--
-- sent_at is stored in SQLite space form ('YYYY-MM-DD HH:MM:SS'), NOT rfc3339.
-- list_runtime_activity keeps any row whose sent_at fails rfc3339 parsing
-- (unwrap_or(true)), so the activity rows stay visible regardless of the wall
-- clock on the day the automation runs, while remaining fully deterministic.
INSERT INTO notification_logs (id, event_name, task_id, agent_id, sent_at, channels, error) VALUES
  ('seed-log-completed',
   'task.completed',
   'seed-task-alpha-completed',
   'seed-agent',
   '2026-07-01 06:00:20',
   '{"seed-channel-desktop":"ok"}',
   NULL),
  ('seed-log-failed',
   'task.failed',
   'seed-task-echo-failed',
   'seed-agent',
   '2026-07-01 02:00:15',
   '{"seed-channel-desktop":"ok"}',
   NULL),
  ('seed-log-trigger-error',
   'trigger.error',
   NULL,
   'seed-agent',
   '2026-07-01 02:30:00',
   '{}',
   'seeded trigger error for the activity tab'),
  ('seed-log-degraded',
   'agent.degraded',
   NULL,
   'seed-agent',
   '2026-07-01 03:00:00',
   '{"seed-channel-desktop":"ok"}',
   NULL),
  ('seed-log-llm-down',
   'llm.backend_down',
   NULL,
   NULL,
   '2026-07-01 03:30:00',
   '{}',
   'seeded backend outage for the activity tab'),
  ('seed-log-input-required',
   'task.input_required',
   'seed-task-charlie-approval',
   'seed-agent',
   '2026-07-01 04:00:05',
   '{"seed-channel-desktop":"ok"}',
   NULL);
