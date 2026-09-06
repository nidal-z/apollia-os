-- Opt-in seed fragment for agents.db: the orchestrated planner.
-- Applied by build-seed.sh only when APOLLIA_SEED_PLANNER=1, after the base
-- fragments and before the install_path rewrite (step 3), so the row is
-- rewritten like every other one. The base fixture never carries it: the CLI
-- suite and the -det books assert four agents.
--
-- Why it exists: the run-keyed plan gate (RuntimeEvent::PlanApprovalRequired,
-- crates/apollia-oria/src/engine/direct.rs await_plan_gate) is reached only
-- through ORIAEngine::execute, which the desktop enters for a TASK of an agent
-- whose manifest execution_mode is "orchestrated"
-- (crates/apollia-desktop/src/backend/runner.rs, ExecutionBackend::execute).
-- No base agent is orchestrated. The backend reads execution_mode and
-- system_prompt from THIS row (bootstrap.rs auto_load_installed_agents hands
-- agent.manifest to the factory), and the orchestrated path refuses a manifest
-- without system_prompt (engine/orchestrated.rs), so both live here as well as
-- in files/agents/seed-planner/agent.py, which the loader validates.
--
-- The system prompt pins the plan shape the plan-gate-llm book relies on:
-- three linear reasoning steps, no tool, so the approved run executes LLM
-- steps only (no HITL card, no file written under the seeded HOME).

INSERT INTO installed_agents
    (name, version, install_path, source_path, manifest_json, enabled, installed_at, updated_at)
VALUES
    ('seed-planner', '0.1.0',
     '/__SEED_HOME__/.apollia/agents/seed-planner',
     '/__SEED_HOME__/.apollia/agents/seed-planner/agent.py',
     '{"name":"seed-planner","version":"0.1.0","description":"Orchestrated planner worker: every task goes through an ORIA plan and its approval gate.","tools_required":[],"tools_optional":[],"supports_streaming":false,"supports_a2a":false,"memory_namespace":"seed-planner","shared_memory_namespaces":[],"max_concurrent_tasks":1,"tags":["worker","planner","seed"],"skills":[],"execution_mode":"orchestrated","system_prompt":"You are the seed planner of the Apollia automation suite. Every plan you produce has exactly three steps, s1, s2 and s3, each a pure reasoning step with tool_hint \"llm\" and no args. s2 depends on s1 and s3 depends on s2. Each description is one short sentence. Never call a tool.","agent_type":"worker","examples":["Draft the agenda of the weekly team meeting"],"limitations":["Reasoning steps only, never calls a tool"],"setup_notes":"Fire its trigger from the Automations page; the plan waits for approval."}',
     1, '2026-07-16T09:00:00Z', '2026-07-16T09:00:00Z');
