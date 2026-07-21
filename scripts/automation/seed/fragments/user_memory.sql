-- Deterministic seed fragment for the MEMORY subsystem (per-namespace *.db).
-- Tables: semantic_memories, episodic_memories, procedural_memories, memory_fts.
-- Schema is applied separately by the builder (schemas/user_memory.sql).
--
-- Runtime model (traced from the read path):
--   * IPC list_memory_namespaces() scans ~/.apollia/memory/*.db and returns the
--     file stems: one .db FILE == one namespace. The file name IS the namespace.
--   * IPC list_memory_entries(ns) opens <ns>.db and unions:
--       - episodic:   EpisodicMemory::history  (WHERE namespace = ns, ORDER created_at DESC)
--       - semantic:   SemanticMemory::recall_all(WHERE namespace = ns AND not expired, ORDER key ASC)
--       - procedural: ProceduralMemory::list    (WHERE namespace = ns, ORDER success_count DESC)
--   * IPC search_memory(ns, q) runs FTS5 over memory_fts (episodic + semantic only).
--
-- Namespace -> category mapping (crates/apollia-desktop/ui/src/routes/Memory.svelte
-- classifyNamespace + NamespaceSidebar categories):
--   * "__user__"                       -> user    (shows the profile banner)
--   * name contains ":"                -> project (e.g. "seed-project-alpha:default")
--   * name equals an installed agent   -> agent   (bundled: "apollia-guide", "onboarding-agent")
--   * anything else                    -> other   (e.g. "legacy-notes")
--
-- This single fragment carries the rows of ALL seed namespaces. The builder
-- writes each namespace to its own <ns>.db (see files/memory/build.sh); because
-- every read query filters by namespace, applying the whole fragment to one db
-- (used by the test below) or to each per-namespace db is equivalent.
--
-- Determinism: fixed ids, fixed timestamp 2026-07-01T00:00:00Z, no random values.
-- Embeddings: this schema has NO embedding/vector column, so none is seeded.
-- FTS: no triggers exist in the schema; the runtime inserts memory_fts rows in
--   Rust, so this fragment inserts them explicitly (content mirrors the Rust
--   code: episodic -> raw content, semantic -> "<key> <value_str>").

-- Stamp the schema version so MemoryStore::open() skips re-migration.
INSERT INTO _schema_version (version) VALUES (2);

-- ============================================================================
-- NAMESPACE: __user__            (category: user)  -- richest namespace: it is
-- the first sidebar item, so the automation selects it and walks every type.
-- ============================================================================
INSERT INTO semantic_memories (id, namespace, key, value, source, confidence, created_at, updated_at, expires_at) VALUES
  ('sem-user-tone',   '__user__', 'preferences.tone',              '"Concis, technique, sans remplissage"', 'onboarding', 1.0, '2026-07-01T00:00:00Z', '2026-07-01T00:00:00Z', NULL),
  ('sem-user-lang',   '__user__', 'preferences.language',          '"fr"',                            'onboarding', 1.0, '2026-07-01T00:00:00Z', '2026-07-01T00:00:00Z', NULL),
  ('sem-user-budget', '__user__', 'constraints.monthly_budget_eur', '150',                            'user',       0.9, '2026-07-01T00:00:00Z', '2026-07-01T00:00:00Z', '2099-01-01T00:00:00Z');
INSERT INTO episodic_memories (id, namespace, task_id, agent_id, content, importance, created_at, expires_at, metadata) VALUES
  ('ep-user-01', '__user__', NULL, 'apollia-guide', 'L''utilisateur a confirmé son profil lors de l''onboarding.', 0.7, '2026-07-01T00:00:00Z', NULL, '{}'),
  ('ep-user-02', '__user__', NULL, 'apollia-guide', 'L''utilisateur préfère des réponses courtes.',                   0.6, '2026-07-01T00:00:00Z', NULL, '{}');
INSERT INTO procedural_memories (id, namespace, trigger_text, steps, success_count, last_used_at, created_at) VALUES
  ('proc-user-01', '__user__', 'quand l''utilisateur salue', '["détecter la langue","répondre en français","proposer de laide"]', 4, '2026-07-01T00:00:00Z', '2026-07-01T00:00:00Z');

-- ============================================================================
-- NAMESPACE: apollia-guide       (category: agent, bundled agent)
-- ============================================================================
INSERT INTO semantic_memories (id, namespace, key, value, source, confidence, created_at, updated_at, expires_at) VALUES
  ('sem-guide-topic', 'apollia-guide', 'last_topic', '"sous-système mémoire"', 'apollia-guide', 0.85, '2026-07-01T00:00:00Z', '2026-07-01T00:00:00Z', NULL),
  ('sem-guide-mode',  'apollia-guide', 'ui.mode',    '"operator"',         'apollia-guide', 0.8,  '2026-07-01T00:00:00Z', '2026-07-01T00:00:00Z', NULL);
INSERT INTO episodic_memories (id, namespace, task_id, agent_id, content, importance, created_at, expires_at, metadata) VALUES
  ('ep-guide-01', 'apollia-guide', NULL, 'apollia-guide', 'Explication de l''explorateur de mémoire à l''utilisateur.', 0.6, '2026-07-01T00:00:00Z', NULL, '{}'),
  ('ep-guide-02', 'apollia-guide', NULL, 'apollia-guide', 'Accompagnement du téléchargement d''un modèle.',      0.5, '2026-07-01T00:00:00Z', NULL, '{}');
INSERT INTO procedural_memories (id, namespace, trigger_text, steps, success_count, last_used_at, created_at) VALUES
  ('proc-guide-01', 'apollia-guide', 'quand on interroge sur une fonctionnalité', '["localiser la fonctionnalité","résumer son rôle","créer un lien vers la page"]', 7, '2026-07-01T00:00:00Z', '2026-07-01T00:00:00Z');

-- ============================================================================
-- NAMESPACE: onboarding-agent    (category: agent, bundled agent)
-- ============================================================================
INSERT INTO semantic_memories (id, namespace, key, value, source, confidence, created_at, updated_at, expires_at) VALUES
  ('sem-onb-step', 'onboarding-agent', 'progress.step', '"tier-2"', 'onboarding-agent', 0.9, '2026-07-01T00:00:00Z', '2026-07-01T00:00:00Z', NULL);
INSERT INTO episodic_memories (id, namespace, task_id, agent_id, content, importance, created_at, expires_at, metadata) VALUES
  ('ep-onb-01', 'onboarding-agent', NULL, 'onboarding-agent', 'Collecte des informations de base du profil.', 0.5, '2026-07-01T00:00:00Z', NULL, '{}');
INSERT INTO procedural_memories (id, namespace, trigger_text, steps, success_count, last_used_at, created_at) VALUES
  ('proc-onb-01', 'onboarding-agent', 'au premier lancement', '["accueillir","collecter le profil","suggérer un premier agent"]', 3, '2026-07-01T00:00:00Z', '2026-07-01T00:00:00Z');

-- ============================================================================
-- NAMESPACE: seed-project-alpha:default   (category: project, ":" present)
-- project_id "seed-project-alpha" matches the projects.sql seed for a clean
-- displayName ("default . seed-project-alpha").
-- ============================================================================
INSERT INTO semantic_memories (id, namespace, key, value, source, confidence, created_at, updated_at, expires_at) VALUES
  ('sem-proj-stack',  'seed-project-alpha:default', 'project.stack',           '"Rust + Svelte"', 'apollia-guide', 0.9, '2026-07-01T00:00:00Z', '2026-07-01T00:00:00Z', NULL),
  ('sem-proj-budget', 'seed-project-alpha:default', 'client.dupont.budget_eur', '15000',          'user',          0.8, '2026-07-01T00:00:00Z', '2026-07-01T00:00:00Z', NULL);
INSERT INTO episodic_memories (id, namespace, task_id, agent_id, content, importance, created_at, expires_at, metadata) VALUES
  ('ep-proj-01', 'seed-project-alpha:default', 'task-alpha-1', 'apollia-guide', 'Rédaction de la note de cadrage du projet client.', 0.7, '2026-07-01T00:00:00Z', NULL, '{}'),
  ('ep-proj-02', 'seed-project-alpha:default', 'task-alpha-2', 'apollia-guide', 'Revue de la configuration du fournisseur.',              0.6, '2026-07-01T00:00:00Z', NULL, '{}');
INSERT INTO procedural_memories (id, namespace, trigger_text, steps, success_count, last_used_at, created_at) VALUES
  ('proc-proj-01', 'seed-project-alpha:default', 'à l''arrivée d''un nouveau document', '["indexer le document","extraire les entités","mettre à jour la synthèse"]', 5, '2026-07-01T00:00:00Z', '2026-07-01T00:00:00Z');

-- ============================================================================
-- NAMESPACE: legacy-notes        (category: other, no ":" and not an agent)
-- ============================================================================
INSERT INTO semantic_memories (id, namespace, key, value, source, confidence, created_at, updated_at, expires_at) VALUES
  ('sem-legacy-01', 'legacy-notes', 'migration.note', '"Importé depuis lancien système de notes"', NULL, 0.6, '2026-07-01T00:00:00Z', '2026-07-01T00:00:00Z', NULL);
INSERT INTO episodic_memories (id, namespace, task_id, agent_id, content, importance, created_at, expires_at, metadata) VALUES
  ('ep-legacy-01', 'legacy-notes', NULL, 'legacy-import', 'Note héritée archivée sans agent propriétaire.', 0.4, '2026-07-01T00:00:00Z', NULL, '{}');
INSERT INTO procedural_memories (id, namespace, trigger_text, steps, success_count, last_used_at, created_at) VALUES
  ('proc-legacy-01', 'legacy-notes', 'à l''archivage', '["compresser","stocker","journaliser"]', 2, '2026-07-01T00:00:00Z', '2026-07-01T00:00:00Z');

-- ============================================================================
-- FTS5 index rows (memory_fts). Mirrors the Rust write path:
--   episodic  -> content = raw content,             source_table = 'episodic'
--   semantic  -> content = "<key> <value_str>",     source_table = 'semantic'
--   procedural-> not indexed (no FTS in the runtime).
-- ============================================================================
INSERT INTO memory_fts (content, source_table, source_id) VALUES
  ('L''utilisateur a confirmé son profil lors de l''onboarding.', 'episodic', 'ep-user-01'),
  ('L''utilisateur préfère des réponses courtes.',                   'episodic', 'ep-user-02'),
  ('Explication de l''explorateur de mémoire à l''utilisateur.',       'episodic', 'ep-guide-01'),
  ('Accompagnement du téléchargement d''un modèle.',            'episodic', 'ep-guide-02'),
  ('Collecte des informations de base du profil.',                  'episodic', 'ep-onb-01'),
  ('Rédaction de la note de cadrage du projet client.',   'episodic', 'ep-proj-01'),
  ('Revue de la configuration du fournisseur.',                'episodic', 'ep-proj-02'),
  ('Note héritée archivée sans agent propriétaire.',       'episodic', 'ep-legacy-01');
INSERT INTO memory_fts (content, source_table, source_id) VALUES
  ('preferences.tone "Concis, technique, sans remplissage"',            'semantic', 'sem-user-tone'),
  ('preferences.language "fr"',                                   'semantic', 'sem-user-lang'),
  ('constraints.monthly_budget_eur 150',                          'semantic', 'sem-user-budget'),
  ('last_topic "sous-système mémoire"',                               'semantic', 'sem-guide-topic'),
  ('ui.mode "operator"',                                          'semantic', 'sem-guide-mode'),
  ('progress.step "tier-2"',                                      'semantic', 'sem-onb-step'),
  ('project.stack "Rust + Svelte"',                               'semantic', 'sem-proj-stack'),
  ('client.dupont.budget_eur 15000',                              'semantic', 'sem-proj-budget'),
  ('migration.note "Importé depuis lancien système de notes"',    'semantic', 'sem-legacy-01');
