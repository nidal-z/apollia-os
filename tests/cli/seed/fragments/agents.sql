-- Seed fragment for agents.db (schema: schemas/agents.sql).
-- Deterministic. INSERTs only. English. No PM tokens.
--
-- install_path / source_path / root_path below are PLACEHOLDERS.
-- build-seed.sh rewrites them to the real seed location:
--   installed_agents.install_path -> <SEED_HOME>/.apollia/agents/<name>
--   installed_packages.root_path  -> <SEED_HOME>/.apollia/agents/packages/<name>
-- The on-disk agent dirs live under files/agents/<name>/ and are copied
-- verbatim into <SEED_HOME>/.apollia/agents/ by the builder.
--
-- Read path exercised by this fragment:
--   list_agents (commands/agents.rs)        -> agent-list-row-*, agent-detail-*
--   list_agent_packages (agent_packages.rs) -> package-list-row-*
-- manifest_json is deserialized into apollia_core::AgentManifest, so every
-- object below is a valid, parseable AgentManifest (required keys: name,
-- version, description, tools_required; all others default).

-- ── Installed agents ────────────────────────────────────────────────────────

INSERT INTO installed_agents
    (name, version, install_path, source_path, manifest_json, enabled, installed_at, updated_at)
VALUES
    ('apollia-chat', '1.0.0',
     '/__SEED_HOME__/.apollia/agents/apollia-chat',
     '/__SEED_HOME__/.apollia/agents/apollia-chat/agent.py',
     '{"name":"apollia-chat","version":"1.0.0","description":"Apollia free-chat assistant: general-purpose conversational agent with tool access.","tools_required":[],"tools_optional":["web_search","file_read"],"supports_streaming":true,"supports_a2a":false,"memory_namespace":"apollia-chat","shared_memory_namespaces":[],"max_concurrent_tasks":1,"tags":["chat","assistant","system"],"skills":[],"execution_mode":"auto","system_prompt":"You are the Apollia free-chat assistant. Answer clearly and use tools when helpful.","agent_type":"system","examples":["Summarize this document","Draft a reply to this email"],"limitations":["Does not run long background jobs","No access to external accounts unless a connector is configured"],"setup_notes":"No setup required. Optionally connect tools to extend capabilities."}',
     1, '2026-07-16T09:00:00Z', '2026-07-16T09:00:00Z');

INSERT INTO installed_agents
    (name, version, install_path, source_path, manifest_json, enabled, installed_at, updated_at)
VALUES
    ('onboarding-agent', '0.1.0-preview',
     '/__SEED_HOME__/.apollia/agents/onboarding-agent',
     '/__SEED_HOME__/.apollia/agents/onboarding-agent/agent.py',
     '{"name":"onboarding-agent","version":"0.1.0-preview","description":"First user contact: short calibration on identity, supervision and sovereignty.","tools_required":["permission_rule_add","permission_rule_list"],"tools_optional":[],"supports_streaming":false,"supports_a2a":false,"memory_namespace":"onboarding","shared_memory_namespaces":[],"max_concurrent_tasks":1,"tags":["onboarding","conversational","system"],"skills":[],"execution_mode":"conversational","agent_type":"system","user_memory_write":true,"examples":["Start onboarding"],"limitations":["Runs once at first launch"]}',
     1, '2026-07-16T09:00:00Z', '2026-07-16T09:00:00Z');

INSERT INTO installed_agents
    (name, version, install_path, source_path, manifest_json, enabled, installed_at, updated_at)
VALUES
    ('apollia-guide', '0.1.0-preview',
     '/__SEED_HOME__/.apollia/agents/apollia-guide',
     '/__SEED_HOME__/.apollia/agents/apollia-guide/agent.py',
     '{"name":"apollia-guide","version":"0.1.0-preview","description":"Conversational coach for Apollia OS: knows product capabilities and suggests actionable deep-links.","tools_required":[],"tools_optional":["navigate","read_memory_namespace","get_user_integrations","get_installed_agents"],"supports_streaming":false,"supports_a2a":false,"memory_namespace":"apollia-guide","shared_memory_namespaces":[],"max_concurrent_tasks":1,"tags":["coach","system","guide"],"skills":[],"execution_mode":"conversational","agent_type":"assistant","examples":["What can Apollia do?","How do I install an agent?"],"limitations":["Guidance only, does not act on your behalf"]}',
     1, '2026-07-16T09:00:00Z', '2026-07-16T09:00:00Z');

INSERT INTO installed_agents
    (name, version, install_path, source_path, manifest_json, enabled, installed_at, updated_at)
VALUES
    ('seed-classifier', '0.1.0',
     '/__SEED_HOME__/.apollia/agents/seed-classifier',
     '/__SEED_HOME__/.apollia/agents/seed-classifier/agent.py',
     '{"name":"seed-classifier","version":"0.1.0","description":"Deterministic text classifier worker: routes text into labeled categories via an A2A skill.","tools_required":[],"tools_optional":["llm"],"supports_streaming":false,"supports_a2a":true,"memory_namespace":"seed-classifier","shared_memory_namespaces":[],"max_concurrent_tasks":2,"tags":["worker","nlp","classification"],"skills":[{"id":"classify_text","name":"Classify text","description":"Classify a text into one of the provided labels.","input_modes":["data"],"output_modes":["data"],"examples":[],"input_schema":{"text":{"type":"string","description":"Text to classify","required":true},"labels":{"type":"array","description":"Candidate labels","required":true}}}],"execution_mode":"direct","agent_type":"worker","examples":["Classify this ticket as bug or feature"],"limitations":["Single-label output","Labels must be provided by the caller"],"setup_notes":"Provide candidate labels in each request."}',
     1, '2026-07-16T09:00:00Z', '2026-07-16T09:00:00Z');

-- ── Installed package + agent link ──────────────────────────────────────────
-- list_agent_packages reads manifest_json.package.description and
-- manifest_json.agents[] (name/role/entry). package_agents links the package to
-- its installed agent (used by uninstall + list_agents_for_package).

INSERT INTO installed_packages
    (name, version, root_path, manifest_json, installed_at, updated_at)
VALUES
    ('seed-office-pack', '0.1.0',
     '/__SEED_HOME__/.apollia/agents/packages/seed-office-pack',
     '{"package":{"name":"seed-office-pack","version":"0.1.0","description":"Seed office toolkit: bundled workers for classification tasks.","author":"Apollia <contact@apollia.fr>"},"agents":[{"name":"seed-classifier","role":"worker","entry":"agents/seed_classifier/agent.py"}]}',
     '2026-07-16T09:00:00Z', '2026-07-16T09:00:00Z');

INSERT INTO package_agents (package_name, agent_name)
VALUES ('seed-office-pack', 'seed-classifier');
