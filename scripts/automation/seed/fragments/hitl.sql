-- Deterministic seed fragment for the TASKS / HITL subsystem (hitl.db).
-- Tables: tasks, task_approvals. Schema is applied separately by the builder
-- (schemas/hitl.sql).
--
-- Unblocks the desktop Tasks + Inbox automation:
--   * task-row-* / tasks-split-row-* (Tasks.svelte, list_tasks -> persisted
--     TaskRepository.list_recent_tasks)                       <- tasks
--   * the status filter chips tasks-filter-{all,todo,running,approval,done,error}
--     each get >=1 match; the chip predicate reads the status DERIVED from the
--     last entry of transitions_json, NOT the status column:
--        todo=submitted  running=working  approval=input_required
--        done=completed  error=failed
--     so both transitions_json and the status column are set consistently.
--   * the detail tabs (overview/output/trace/technical): input_text feeds
--     task-overview-input, output_text feeds task-overview-output, and
--     transitions_json feeds the trace timeline.
--   * task_approvals: a pending row (approved IS NULL) backs get_approval_info /
--     the Inbox pending enrichment; a resolved row backs list_resolved_approvals
--     and the task timeline HITL events.
--
-- IMPORTANT read-path note (residual gap, see the session report): the Inbox
-- "pending" tab (inbox-row-primary-*) is populated by list_pending_approvals,
-- which reads the IN-MEMORY PendingApprovals set (pending_task_ids), not this
-- table. A cold seed launch has an empty in-memory set, so the pending rows are
-- not rendered from the DB alone. The input_required task + pending approval
-- below still unblock the Tasks "approval" filter and make get_approval_info
-- resolve correctly if the id ever enters the in-memory set.
--
-- Deterministic: fixed ids, timestamps anchored on 2026-07-01. created_at is
-- given distinct hours purely so ORDER BY created_at DESC is stable (the most
-- recent row, seed-task-alpha-completed, is nth 0 and carries an output).

-- Tasks. list_recent_tasks selects
--   task_id, agent_name, input_text, output_text, duration_ms,
--   transitions_json, created_at
-- ORDER BY created_at DESC LIMIT 50. Status is derived from transitions_json.
INSERT INTO tasks
  (task_id, agent_name, status,
   input_required_prompt, input_required_context,
   input_text, input_truncated, output_text, output_truncated,
   duration_ms, transitions_json, created_at, updated_at)
VALUES
  ('seed-task-alpha-completed',
   'Agent Recherche',
   'completed',
   NULL, NULL,
   'Résumer la veille marché de la semaine pour le tableau de bord.',
   0,
   'Synthèse : 7 sources analysées, 3 signaux prioritaires retenus, aucun risque bloquant détecté.',
   0,
   4200,
   '[{"status":"submitted","ts":"2026-07-01T06:00:00Z"},{"status":"working","ts":"2026-07-01T06:00:05Z"},{"status":"completed","ts":"2026-07-01T06:00:20Z"}]',
   '2026-07-01T06:00:00Z',
   '2026-07-01T06:00:20Z'),
  ('seed-task-bravo-working',
   'Agent Recherche',
   'working',
   NULL, NULL,
   'Surveiller en continu la latence de livraison du canal webhook.',
   0,
   NULL,
   0,
   NULL,
   '[{"status":"submitted","ts":"2026-07-01T05:00:00Z"},{"status":"working","ts":"2026-07-01T05:00:05Z"}]',
   '2026-07-01T05:00:00Z',
   '2026-07-01T05:00:05Z'),
  ('seed-task-charlie-approval',
   'Agent Opérations',
   'input_required',
   'Approuver l''envoi du livrable au client ?',
   '{"risk":{"level":"medium","summary":"Approbation en attente dune décision de lopérateur","impact":"Envoie le livrable trimestriel au client","consequences":["Action externe : envoi effectif au client"],"rationale":"Palier de validation humaine avant tout envoi"}}',
   'Préparer et livrer le digest trimestriel, en attente de validation humaine.',
   0,
   NULL,
   0,
   NULL,
   '[{"status":"submitted","ts":"2026-07-01T04:00:00Z"},{"status":"working","ts":"2026-07-01T04:00:03Z"},{"status":"input_required","ts":"2026-07-01T04:00:05Z"}]',
   '2026-07-01T04:00:00Z',
   '2026-07-01T04:00:05Z'),
  ('seed-task-delta-submitted',
   'Agent Opérations',
   'submitted',
   NULL, NULL,
   'Reconstituer l''historique des notifications manquantes.',
   0,
   NULL,
   0,
   NULL,
   '[{"status":"submitted","ts":"2026-07-01T03:00:00Z"}]',
   '2026-07-01T03:00:00Z',
   '2026-07-01T03:00:00Z'),
  ('seed-task-echo-failed',
   'Agent Recherche',
   'failed',
   NULL, NULL,
   'Récupérer le manifeste distant depuis un point d''accès injoignable.',
   0,
   'Erreur : connexion à https://example.invalid/webhook refusée (point d''accès injoignable).',
   0,
   1800,
   '[{"status":"submitted","ts":"2026-07-01T02:00:00Z"},{"status":"working","ts":"2026-07-01T02:00:04Z"},{"status":"failed","ts":"2026-07-01T02:00:15Z"}]',
   '2026-07-01T02:00:00Z',
   '2026-07-01T02:00:15Z'),
  ('seed-task-foxtrot-completed',
   'Agent Opérations',
   'completed',
   NULL, NULL,
   'Livrer le digest trimestriel précédemment approuvé.',
   0,
   'Digest livré après approbation de l''opérateur.',
   0,
   5100,
   '[{"status":"submitted","ts":"2026-07-01T01:00:00Z"},{"status":"working","ts":"2026-07-01T01:00:04Z"},{"status":"input_required","ts":"2026-07-01T01:00:06Z"},{"status":"working","ts":"2026-07-01T01:05:00Z"},{"status":"completed","ts":"2026-07-01T01:05:10Z"}]',
   '2026-07-01T01:00:00Z',
   '2026-07-01T01:05:10Z'),
  ('seed-task-golf-canceled',
   'Agent Opérations',
   'canceled',
   NULL, NULL,
   'Tâche annulée par l''opérateur avant la fin.',
   0,
   NULL,
   0,
   NULL,
   '[{"status":"submitted","ts":"2026-07-01T00:00:00Z"},{"status":"working","ts":"2026-07-01T00:00:04Z"},{"status":"canceled","ts":"2026-07-01T00:00:09Z"}]',
   '2026-07-01T00:00:00Z',
   '2026-07-01T00:00:09Z');

-- Approvals. The pending row (approved IS NULL) backs the input_required task;
-- get_approval_info LEFT JOINs it on approved IS NULL. The resolved row
-- (approved = 1, responded_at set) backs list_resolved_approvals and the task
-- timeline HITL events.
INSERT INTO task_approvals
  (id, task_id, step_id, prompt, context_json, approved, reason,
   requested_at, responded_at, suspended_at, wait_duration_ms)
VALUES
  ('seed-approval-pending',
   'seed-task-charlie-approval',
   NULL,
   'Approuver l''envoi du livrable au client ?',
   '{"risk":{"level":"medium","summary":"Approbation en attente dune décision de lopérateur"}}',
   NULL,
   NULL,
   '2026-07-01T04:00:05Z',
   NULL,
   '2026-07-01T04:00:05Z',
   NULL),
  ('seed-approval-resolved',
   'seed-task-foxtrot-completed',
   NULL,
   'Approuver la livraison du digest quotidien ?',
   '{"risk":{"level":"low","summary":"Approbation déjà résolue"}}',
   1,
   NULL,
   '2026-07-01T01:00:06Z',
   '2026-07-01T01:05:00Z',
   '2026-07-01T01:00:06Z',
   294000);
