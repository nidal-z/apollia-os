-- Deterministic seed fragment for the ORIA plan cache (plan_cache.db).
-- Table: plan_cache. Schema is applied separately by the builder
-- (schemas/plan_cache.sql, the DDL PlanCacheRepository::open creates).
--
-- Unblocks the desktop Observability > Plan cache tab (PlanCacheStats.svelte):
--   * plan-cache-total / plan-cache-hits / plan-cache-hit-rate / plan-cache-misses
--     render the populated cards instead of the empty state, and
--   * plan-cache-clear-btn opens plan-cache-purge-dialog, whose confirm calls
--     clear_all and brings the empty state back.
--
-- Read path: crates/apollia-oria/src/plan_cache.rs. stats() counts the rows
-- and sums hit_count; lookup() deserializes plan_json as an ExecutionPlan
-- (crates/apollia-oria/src/plan.rs: plan_id, task_id, steps of
-- apollia_core::plan::PlanStep, the same shape chat.sql stores in
-- session_plan_steps.payload).
--
-- cache_key is what compute_cache_key would return for this exact input:
--   SHA-256("seed-classifier:0.1.0:llm:summarize the weekly market watch for the dashboard.")
-- so the row is one the engine could have written, keyed on the agent, its
-- version, its optional tool (agents.sql manifest) and the input text of
-- seed-task-alpha-completed (hitl.sql).
--
-- Timestamps use the SQLite datetime('now') layout the repository writes, so
-- evict_expired's string comparison orders the row like a live one. Nothing in
-- tests/cli counts this table (the CLI suite only asserts that the plan cache
-- commands exit 0), so one row here is not a suite change.
INSERT INTO plan_cache
  (cache_key, plan_json, hit_count, created_at, last_used_at, agent_name, agent_version)
VALUES
  ('65edc903a1d732d3ee515bf68abebc1523357b78adb498ca925a278b23b97fc4',
   '{"plan_id":"seed-plan-cache-1","task_id":"seed-task-alpha-completed","steps":[{"step_id":"seed-cached-step-1","title":"Collect the weekly sources","description":"Gather the seven market watch sources for the dashboard.","status":"completed","depends_on":[],"tool_hint":"llm","model_hint":null,"rationale":"Every summary starts from the full source list.","provenance":{"origin":"initial","reason":null,"at":0}},{"step_id":"seed-cached-step-2","title":"Rank the signals","description":"Keep the priority signals and drop the noise.","status":"completed","depends_on":["seed-cached-step-1"],"tool_hint":"llm","model_hint":null,"rationale":"The dashboard shows three signals at most.","provenance":{"origin":"initial","reason":null,"at":0}},{"step_id":"seed-cached-step-3","title":"Write the summary","description":"Produce the dashboard summary from the ranked signals.","status":"completed","depends_on":["seed-cached-step-2"],"tool_hint":"llm","model_hint":null,"rationale":"Final deliverable of the task.","provenance":{"origin":"initial","reason":null,"at":0}}]}',
   3,
   '2026-07-01 06:00:00',
   '2026-07-01 06:30:00',
   'seed-classifier',
   '0.1.0');
