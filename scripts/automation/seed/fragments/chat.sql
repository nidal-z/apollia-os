-- Deterministic seed fragment for the CHAT subsystem (chat.db).
--
-- Unblocks the chat-det automation script:
--   * chat_sessions  -> conversation-row-* / chat-active-list is populated,
--                        so the session list is not the empty state.
--   * chat_messages  -> seed-session-1 renders a real conversation history.
--   * chat_tool_authorizations -> session-auths-list in Settings > Permissions
--                        renders (restore_sessions hydrates these in-memory at
--                        boot, then list_active_chat_session_authorizations
--                        surfaces them; only status='active' sessions are
--                        restored, hence every seeded session is 'active').
--   * session_plans + session_plan_steps -> chat-inspector-plan-tab shows a
--                        plan for seed-session-2.
--
-- Read paths verified against:
--   crates/apollia-runtime/src/chat/repository.rs (list_sessions / get_messages
--     / get_authorized_tools),
--   crates/apollia-runtime/src/chat/plan_actor.rs (load_plan / load_steps),
--   crates/apollia-runtime/src/chat/manager/user_input.rs (restore_sessions),
--   crates/apollia-desktop/ui/src/routes/Chat.svelte,
--   crates/apollia-desktop/ui/src/routes/settings/Permissions.svelte.
--
-- Constraints honoured: mode in ('libre','agent','companion'), status in
-- ('active','processing','closed'). No 'processing' rows (they would flip the
-- live indicator and get reset to active on restore). Fixed timestamps, fixed
-- ids. INSERTs only. The chat_sessions_fts* virtual tables are populated by the
-- runtime (summarizer), not by SQLite triggers, so base-table inserts do not
-- touch FTS and do not fail; the session list reads the base table directly.

-- 4 chat sessions, all status='active'. seed-session-3 is linked to a project.
INSERT INTO chat_sessions
    (id, mode, agent_name, system_prompt, status, available_tools, created_at,
     closed_at, llm_backend, summary, title, parent_session_id, fork_depth,
     project_id, plan_mode, plan_phase)
VALUES
    ('seed-session-1', 'libre', NULL, '', 'active',
     '["fs.read","web.fetch"]', '2026-07-01T00:00:00Z',
     NULL, 'local', 'Walkthrough of the sovereign runtime and local inference.',
     'Discovering the sovereign runtime', NULL, 0, NULL, 0, 'done'),
    ('seed-session-2', 'agent', 'apollia-guide', '', 'active',
     '["a2a.delegate","fs.read"]', '2026-07-01T00:00:00Z',
     NULL, 'local', 'Preparing the preview release checklist.',
     'Preparing the release checklist', NULL, 0, NULL, 1, 'done'),
    ('seed-session-3', 'companion', 'apollia-guide', '', 'active',
     '[]', '2026-07-01T00:00:00Z',
     NULL, 'local', 'Companion coaching thread for the alpha project.',
     'Session with the companion', NULL, 0, 'seed-project-alpha', 0, 'done'),
    ('seed-session-4', 'libre', NULL, '', 'active',
     '["mcp.list"]', '2026-07-01T00:00:00Z',
     NULL, 'local', 'Exploring the available MCP tools.',
     'Exploring the MCP tools', NULL, 0, NULL, 0, 'done');

-- Conversation history for seed-session-1 (user + assistant turns).
INSERT INTO chat_messages
    (id, session_id, role, content, tool_calls_json, tool_name, created_at, seq, metadata)
VALUES
    ('seed-msg-1', 'seed-session-1', 'user',
     'How does Apollia keep my data local?', NULL, NULL,
     '2026-07-01T00:00:00Z', 1, NULL),
    ('seed-msg-2', 'seed-session-1', 'assistant',
     'The runtime runs inference on-device and never sends your data off the machine without an explicit action.',
     NULL, NULL, '2026-07-01T00:00:00Z', 2, NULL),
    ('seed-msg-3', 'seed-session-1', 'user',
     'Can it read a local file for me?', NULL, NULL,
     '2026-07-01T00:00:00Z', 3, NULL),
    ('seed-msg-4', 'seed-session-1', 'assistant',
     'Yes, once you authorize the fs.read tool for this session it can read files you point it to.',
     NULL, NULL, '2026-07-01T00:00:00Z', 4, NULL);

-- Session-scoped tool authorizations (surfaced by session-auths-list).
INSERT INTO chat_tool_authorizations (session_id, tool_name, authorized_at)
VALUES
    ('seed-session-1', 'fs.read', '2026-07-01T00:00:00Z'),
    ('seed-session-1', 'web.fetch', '2026-07-01T00:00:00Z'),
    ('seed-session-2', 'a2a.delegate', '2026-07-01T00:00:00Z');

-- Plan for seed-session-2 (chat-inspector-plan-tab).
INSERT INTO session_plans
    (session_id, plan_id, revision, status, summary, updated_at)
VALUES
    ('seed-session-2', 'seed-plan-1', 1, 'executing',
     'Assemble the preview release checklist.', '2026-07-01T00:00:00Z');

-- Plan steps. payload is a JSON PlanStep (apollia_core::plan::PlanStep).
INSERT INTO session_plan_steps (session_id, step_id, ordinal, payload)
VALUES
    ('seed-session-2', 'seed-step-1', 0,
     '{"step_id":"seed-step-1","title":"Gather release notes","description":"Collect the changelog entries for the preview build.","status":"completed","depends_on":[],"tool_hint":null,"model_hint":null,"rationale":"Baseline for the checklist.","provenance":{"origin":"initial","reason":null,"at":0}}'),
    ('seed-session-2', 'seed-step-2', 1,
     '{"step_id":"seed-step-2","title":"Verify the build passes","description":"Run the workspace test suite before tagging the release.","status":"in_progress","depends_on":["seed-step-1"],"tool_hint":null,"model_hint":null,"rationale":"No release without a green build.","provenance":{"origin":"initial","reason":null,"at":0}}'),
    ('seed-session-2', 'seed-step-3', 2,
     '{"step_id":"seed-step-3","title":"Publish the preview tag","description":"Tag and publish the v0.1.0-preview build.","status":"pending","depends_on":["seed-step-2"],"tool_hint":null,"model_hint":null,"rationale":"Final delivery step.","provenance":{"origin":"initial","reason":null,"at":0}}');

-- chat_approval_log: the Inbox "Recent history (last 14 days)" block.
--
-- The table was created empty, and InboxPendingTab only renders the history at
-- all when a pending item exists, so this half of the screen had never been
-- seen. Timestamps are relative for the same reason as audit.sql: the 14-day
-- window would drop anything pinned to a fixed date.
--
-- One of each decision, because the row renders a different icon and label per
-- decision (Approved / Always approved / Rejected), and a rejection carries the
-- reason the operator typed.
INSERT INTO chat_approval_log
  (session_id, message_id, tool_name, decision, resolved_at, reason)
VALUES
  ('seed-session-libre-1', 'seed-msg-1', 'file_write',    'accept',
   strftime('%Y-%m-%dT%H:%M:%SZ', 'now', '-2 hours'),  NULL),
  ('seed-session-libre-1', 'seed-msg-2', 'file_read',     'always_accept',
   strftime('%Y-%m-%dT%H:%M:%SZ', 'now', '-5 hours'),  NULL),
  ('seed-session-libre-1', 'seed-msg-3', 'bash_executor', 'refuse',
   strftime('%Y-%m-%dT%H:%M:%SZ', 'now', '-1 day'),    'Outside the perimeter allowed for this workspace.'),
  ('seed-session-agent-1', 'seed-msg-4', 'web_search',    'accept',
   strftime('%Y-%m-%dT%H:%M:%SZ', 'now', '-3 days'),   NULL);
