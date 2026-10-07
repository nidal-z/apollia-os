-- Opt-in seed fragment for agents.db: the planner.
-- Applied by build-seed.sh only when APOLLIA_SEED_PLANNER=1, after the base
-- fragments and before the path rewrite (step 3). The base fixture never
-- carries it: the CLI suite and the -det books assert four agents.
--
-- Why it exists: an agent that plans every task before acting on it. The row
-- is written in the shape of the previous version, as the base rows are; the
-- runtime migrates it into an agent that is data, whose mission is the
-- system prompt below. The override that follows makes it plan always, which
-- is what the previous version read from execution_mode.
--
-- The system prompt pins the plan shape: three linear reasoning steps, no
-- tool, so a run executes LLM steps only (no HITL card, no file written under
-- the seeded HOME).

INSERT INTO installed_agents
    (name, version, install_path, source_path, manifest_json, enabled, installed_at, updated_at)
VALUES
    ('seed-planner', '0.1.0',
     '', '',
     '{"name":"seed-planner","version":"0.1.0","description":"Orchestrated planner worker: every task goes through an ORIA plan and its approval gate.","tools_required":[],"tools_optional":[],"supports_streaming":false,"supports_a2a":false,"memory_namespace":"seed-planner","shared_memory_namespaces":[],"max_concurrent_tasks":1,"tags":["worker","planner","seed"],"skills":[],"execution_mode":"orchestrated","system_prompt":"You are the seed planner of the Apollia automation suite. Every plan you produce has exactly three steps, s1, s2 and s3, each a pure reasoning step with tool_hint \"llm\" and no args. s2 depends on s1 and s3 depends on s2. Each description is one short sentence. Never call a tool.","agent_type":"worker","examples":["Draft the agenda of the weekly team meeting"],"limitations":["Reasoning steps only, never calls a tool"],"setup_notes":"Fire its trigger from the Automations page; the plan waits for approval."}',
     1, '2026-07-16T09:00:00Z', '2026-07-16T09:00:00Z');

INSERT INTO agent_overrides (agent, field, value_json, updated_at)
VALUES ('seed-planner', 'planning', '"always"', '2026-07-16T09:00:00Z');
