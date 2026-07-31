-- llm_calls.db seed: recent LLM usage so the cost aggregate (GET /api/v1/llm/costs,
-- default 7-day window) and the builder-mode llm-stats-table / observability
-- llm-costs chart render non-empty. created_at is relative because the aggregate
-- filters on a rolling recent window; the seed is rebuilt per run.
--
-- strftime with the explicit ISO form, never datetime(): the global timeline
-- compares created_at against a cutoff formatted '%Y-%m-%dT%H:%M:%SZ' and the
-- comparison is lexicographic, so a datetime() value ('2026-07-31 12:43:28')
-- has a space where a 'T' is expected and sorts below every cutoff. The cost
-- aggregate parses instead of comparing, which is why it rendered while the
-- timeline stayed empty on the same rows.
INSERT INTO llm_calls (id, task_id, step_id, backend, model, prompt_tokens, completion_tokens, cost_usd, latency_ms, prompt_text, completion_text, created_at) VALUES
 ('seed-call-1','seed-task-done-1','step-1','local-qwen','Qwen3.6-35B-A3B-MXFP4_MOE.gguf',1200, 340,0.0,   820, 'seed prompt 1','seed completion 1', strftime('%Y-%m-%dT%H:%M:%SZ','now','-2 hours')),
 ('seed-call-2','seed-task-done-1','step-2','local-qwen','Qwen3.6-35B-A3B-MXFP4_MOE.gguf', 640, 180,0.0,   510, 'seed prompt 2','seed completion 2', strftime('%Y-%m-%dT%H:%M:%SZ','now','-6 hours')),
 ('seed-call-3','seed-task-done-2','step-1','openai-gpt4o-mini','gpt-4o-mini',            2200, 900,0.0123,1450,'seed prompt 3','seed completion 3', strftime('%Y-%m-%dT%H:%M:%SZ','now','-1 day')),
 ('seed-call-4','seed-task-run-1', 'step-1','anthropic-claude','claude-3-5-sonnet-latest',3100,1200,0.0456,1980,'seed prompt 4','seed completion 4', strftime('%Y-%m-%dT%H:%M:%SZ','now','-3 days')),
 ('seed-call-5','seed-task-done-2','step-2','openai-gpt4o-mini','gpt-4o-mini',             980, 260,0.0041, 610,'seed prompt 5','seed completion 5', strftime('%Y-%m-%dT%H:%M:%SZ','now','-5 days'));
