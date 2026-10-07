-- Seed fragment for agents.db (schema: schemas/agents.sql).
-- Deterministic. INSERTs only. English. No PM tokens.
--
-- The rows are written in the shape a profile of the previous version holds
-- (tables installed_agents and package_agents, manifest_json as that version
-- stored it). The runtime migrates them when it first opens the file: each
-- row becomes an agent that is data, its definition read from manifest_json
-- (mission from system_prompt or description, access from the tools it
-- named). No Python stands behind an agent, so the seed ships none.
--
-- install_path and source_path are empty: there is no file to name.
-- root_path below is a PLACEHOLDER. build-seed.sh rewrites it to the real
-- seed location:
--   installed_packages.root_path  -> <SEED_HOME>/.apollia/agents/packages/<name>
-- The package folder lives under files/agents/packages/<name>/ and is copied
-- verbatim into <SEED_HOME>/.apollia/agents/ by the builder.
--
-- Read path exercised by this fragment:
--   list_agents (commands/agents.rs)        -> agent-list-row-*, agent-detail-*
--   list_agent_packages (agent_packages.rs) -> package-list-row-*

-- ── Installed agents ────────────────────────────────────────────────────────

INSERT INTO installed_agents
    (name, version, install_path, source_path, manifest_json, enabled, installed_at, updated_at)
VALUES
    ('apollia-chat', '1.0.0',
     '', '',
     '{"name":"apollia-chat","version":"1.0.0","description":"Apollia free-chat assistant: general-purpose conversational agent with tool access.","tools_required":[],"tools_optional":["web_search","file_read"],"supports_streaming":true,"supports_a2a":false,"memory_namespace":"apollia-chat","shared_memory_namespaces":[],"max_concurrent_tasks":1,"tags":["chat","assistant","system"],"skills":[],"execution_mode":"auto","system_prompt":"You are the Apollia free-chat assistant. Answer clearly and use tools when helpful.","agent_type":"system","examples":["Summarize this document","Draft a reply to this email"],"limitations":["Does not run long background jobs","No access to external accounts unless a connector is configured"],"setup_notes":"No setup required. Optionally connect tools to extend capabilities."}',
     1, '2026-07-16T09:00:00Z', '2026-07-16T09:00:00Z');

INSERT INTO installed_agents
    (name, version, install_path, source_path, manifest_json, enabled, installed_at, updated_at)
VALUES
    ('onboarding-agent', '0.2.0-preview',
     '', '',
     '{"name":"onboarding-agent","version":"0.2.0-preview","description":"First user contact: short calibration on identity, supervision and sovereignty.","tools_required":[],"tools_optional":[],"supports_streaming":false,"supports_a2a":false,"memory_namespace":"onboarding","shared_memory_namespaces":[],"max_concurrent_tasks":1,"tags":["onboarding","conversational","system"],"skills":[],"execution_mode":"conversational","agent_type":"system","user_memory_write":true,"examples":["Start onboarding"],"limitations":["Runs once at first launch"]}',
     1, '2026-07-16T09:00:00Z', '2026-07-16T09:00:00Z');

INSERT INTO installed_agents
    (name, version, install_path, source_path, manifest_json, enabled, installed_at, updated_at)
VALUES
    ('apollia-guide', '0.2.0-preview',
     '', '',
     '{"name":"apollia-guide","version":"0.2.0-preview","description":"Conversational coach for Apollia OS: knows product capabilities and suggests actionable deep-links.","tools_required":[],"tools_optional":["navigate","read_memory_namespace","get_user_integrations","get_installed_agents"],"supports_streaming":false,"supports_a2a":false,"memory_namespace":"apollia-guide","shared_memory_namespaces":[],"max_concurrent_tasks":1,"tags":["coach","system","guide"],"skills":[],"execution_mode":"conversational","agent_type":"assistant","examples":["What can Apollia do?","How do I install an agent?"],"limitations":["Guidance only, does not act on your behalf"]}',
     1, '2026-07-16T09:00:00Z', '2026-07-16T09:00:00Z');

INSERT INTO installed_agents
    (name, version, install_path, source_path, manifest_json, enabled, installed_at, updated_at)
VALUES
    ('seed-classifier', '0.1.0',
     '', '',
     '{"name":"seed-classifier","version":"0.1.0","description":"Deterministic text classifier worker: routes text into labeled categories via an A2A skill.","tools_required":[],"tools_optional":["llm"],"supports_streaming":false,"supports_a2a":true,"memory_namespace":"seed-classifier","shared_memory_namespaces":[],"max_concurrent_tasks":2,"tags":["worker","nlp","classification"],"skills":[{"id":"classify_text","name":"Classify text","description":"Classify a text into one of the provided labels.","input_modes":["data"],"output_modes":["data"],"examples":[],"input_schema":{"text":{"type":"string","description":"Text to classify","required":true},"labels":{"type":"array","description":"Candidate labels","required":true}}}],"execution_mode":"direct","agent_type":"worker","examples":["Classify this ticket as bug or feature"],"limitations":["Single-label output","Labels must be provided by the caller"],"setup_notes":"Provide candidate labels in each request."}',
     1, '2026-07-16T09:00:00Z', '2026-07-16T09:00:00Z');

-- ── Installed package + agent link ──────────────────────────────────────────
-- manifest_json is the apollia.toml of the package as JSON
-- (files/agents/packages/seed-office-pack/apollia.toml): its agent is data.
-- package_agents links the package to its agent (used by uninstall +
-- list_agents_for_package). The tool the manifest declares is not linked to
-- the package here, the table that would hold the link being created by the
-- runtime when it migrates the file, so it does not join the tool registry.

INSERT INTO installed_packages
    (name, version, root_path, manifest_json, installed_at, updated_at)
VALUES
    ('seed-office-pack', '0.1.0',
     '/__SEED_HOME__/.apollia/agents/packages/seed-office-pack',
     '{"package":{"name":"seed-office-pack","version":"0.1.0","publisher":"Apollia","description":"Seed office toolkit: bundled workers for classification tasks.","sdk":">=0.2.0-preview"},"agents":[{"id":"seed-classifier","name":"seed-classifier","mission":"Deterministic text classifier worker: routes text into labeled categories.","examples":["Classify this ticket as bug or feature"],"defaults":{"access":["pkg:seed-office-pack/classify_text"]}}],"tools":[{"name":"classify_text","module":"tools/classify.py","description":"Classify a text into one of the provided labels.","action":"read","inputs":{"type":"object","properties":{"text":{"type":"string","description":"Text to classify."},"labels":{"type":"array","items":{"type":"string"},"description":"Candidate labels."}},"required":["text","labels"],"additionalProperties":false}}]}',
     '2026-07-16T09:00:00Z', '2026-07-16T09:00:00Z');

INSERT INTO package_agents (package_name, agent_name)
VALUES ('seed-office-pack', 'seed-classifier');

-- The access of the Apollia agent (`apollia:chat`): the web reading tools, and
-- the file tools of the checkout folder (fragments/system.sql, `seed-checkout`),
-- reading and writing, so the -llama books that read and write under the
-- checkout keep a folder to work in.
INSERT INTO agent_overrides (agent, field, value_json, updated_at)
VALUES ('apollia:chat', 'access',
        '["web_search","web_read","conn:folder:seed-checkout/file_read","conn:folder:seed-checkout/file_list","conn:folder:seed-checkout/file_glob","conn:folder:seed-checkout/file_grep","conn:folder:seed-checkout/file_write","conn:folder:seed-checkout/file_edit"]',
        '2026-07-01T00:00:00Z');
