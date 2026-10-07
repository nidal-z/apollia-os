-- Deterministic seed fragment for the PERMISSIONS / GOVERNANCE subsystem
-- (governance.db). Tables: permission_rules, permission_audit, tool_credentials.
-- Schema is applied separately by the builder (schemas/governance.sql).
--
-- Unblocks the rule assertions of the authorizations screen:
--   * authorizations-rules / authorizations-rule (rules for every agent)
--                          <- permission_rules WHERE scope='global'
--   * authorizations-list-agent, then authorizations-rule on the agent
--                          <- permission_rules WHERE scope='agent' AND agent_id='apollia:chat'
--   * authorizations-revoke-all / -dialog / -confirm
--                          (the confirmation is enabled once a rule exists)
--   * permission_audit is the decision table of an older file: nothing reads
--                          it, and the first open of the file drops it. Its
--                          rows stay here so the suite sees that migration
--                          run on a table that holds data
--   * brave-api-key-delete / -test / -test-result (tools page, web_search)
--                          <- tool_credentials (web_search / brave.api_key)
--
-- Deterministic: fixed ids, fixed timestamps, no random values.
-- Timestamp base: 2026-07-01T00:00:00Z = unix 1782864000 (created_at / decided_at
-- are stored as INTEGER unix seconds, converted to ISO 8601 on the Rust side).

-- Permission rules. Columns action='allow'|'deny', scope IN
-- ('global','project','agent'); 'session' is never persisted. Scope coverage:
-- global x2, project x1, agent x1, so revoke-all always has a target. The agent
-- rule (agent_id='apollia:chat') is the one the screen shows on the built-in agent.
INSERT INTO permission_rules
  (id, tool_name, arg_prefix, action, created_at, created_by, scope, project_path, expires_at, agent_id)
VALUES
  (1, 'bash_executor', 'git', 'allow', 1782864000, 'operator', 'global', NULL, NULL, NULL),
  (2, 'file_edit', NULL, 'allow', 1782864000, 'operator', 'project',
      '__APOLLIA_SEED_WORKSPACE__', NULL, NULL),
  (3, 'web_search', NULL, 'allow', 1782864000, 'apollia:chat', 'agent', NULL, NULL, 'apollia:chat'),
  (4, 'http_fetch', NULL, 'deny', 1782864000, 'operator', 'global', NULL, NULL, NULL);

-- Decision table of an older file (append-only there: UPDATE and DELETE were
-- blocked by triggers). The current schema has no such table; these rows exist
-- to be dropped by the migration.
INSERT INTO permission_audit
  (id, tool_name, first_arg, decision, decided_at, scope, rule_id, agent)
VALUES
  (1, 'bash_executor', 'git status', 'AutoAllowedPrefixRule(1)', 1782864010, 'global', 1, 'apollia:chat'),
  (2, 'file_edit', '__APOLLIA_SEED_WORKSPACE__/src/main.rs', 'AutoAllowedPrefixRule(2)', 1782864020, 'project', 2, NULL),
  (3, 'http_fetch', 'https://example.com', 'AutoDeniedPrefixRule(4)', 1782864030, 'global', 4, 'apollia:chat'),
  (4, 'python_executor', 'print(1)', 'NeedsApproval', 1782864040, NULL, NULL, NULL),
  (5, 'web_search', 'apollia os sovereign runtime', 'AutoAllowedSafeList', 1782864050, 'agent', 3, 'apollia:chat');

-- Tool credential for web_search / brave.api_key. This unblocks the tools-page
-- testids brave-api-key-delete / -test / -test-result, which render whenever the
-- credential exists: governance_list_tools and governance_list_credentials read
-- metadata only (tool_name, key_name, created_at, last_used_at) and never decrypt.
--
-- IMPORTANT: value_encrypted is a PLACEHOLDER blob, not a real AES-256-GCM
-- ciphertext (the master key lives in a per-machine .keyfile in the data dir, so
-- a valid ciphertext cannot be produced deterministically here). Consequences:
--   * the credential is listed, the delete button works, active_backend='brave'.
--   * the live "test" button renders but will report an error (decrypt fails and
--     no live Brave key/network), which is the same surface as an invalid key.
-- The blob is 28 bytes so it clears the "too short" (< 12) length guard in get().
INSERT INTO tool_credentials
  (id, tool_name, key_name, value_encrypted, created_at, last_used_at)
VALUES
  (1, 'web_search', 'brave.api_key',
   X'0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c',
   1782864000, NULL);
