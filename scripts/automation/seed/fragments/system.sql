-- Deterministic seed fragment for the SYSTEM subsystem (system.db).
-- Tables: llm_backends, stt_config.
-- Schema is applied separately by the builder (schemas/system.sql). INSERTs only.
-- Deterministic: fixed names/ids, fixed timestamps, no random values.
--
-- Read path (traced):
--   * llm_backends  -> LlmBackendRepository::list()  (apollia-core/src/llm_backend.rs)
--       SELECT name, provider, model, config_json, enabled, is_default
--       FROM llm_backends ORDER BY name
--     consumed by GET /api/v1/llm/backends -> list_llm_backends IPC -> the
--     `llmBackends` SSE store.
--   * stt_config    -> SttConfigRepository::get_or_default()  (apollia-core/src/stt_config.rs)
--       SELECT enabled, model_path, hotkey, clipboard_mode, clipboard_restore,
--              silence_threshold_db, max_recording_sec, language, trigger_mode
--       FROM stt_config WHERE id = 1
--     consumed by get_stt_config IPC -> the STT settings form.
--
-- Unblocks the desktop automation for llm-det.json / llm-backends.json:
--   * llm-grid / llm-backends-list / llm-backends-section  <- >= 1 backend row
--   * llm-backend-card / llm-backend-badge / llm-backend-type-badge
--   * set-default-{name}  (settings/Llm.svelte, one per backend)
-- and stt-det.json:
--   * stt-config-form / stt-model-select / stt-language-input / stt-trigger /
--     stt-clipboard / stt-silence / stt-max-rec / stt-enable-toggle
--     (settings/Stt.svelte reads the seeded row as a configured state)
--
-- config_json shape mirrors what LlmBackendDialog.svelte writes/reads:
--   llama-cpp -> model_path (basename must match form.model), context_size, temperature
--   api       -> base_url, api_key ("${VAR}" sentinel), context_size, temperature
--
-- Cross-agent coordination (model-hub seed): the local backend below references
-- `Qwen3.6-35B-A3B-MXFP4_MOE.gguf` by basename. model_hub.rs matches installed
-- models to backends/STT by file_name() (tilde-expanded), so this drives the
-- installed-model-inuse-Qwen... badge on the Model Hub page.

-- LLM backends. Sorted by name in the UI (list ORDER BY name). Exactly ONE row
-- has is_default = 1; reload_llm_from_db requires a default backend to exist.
-- The default is an OpenAI-compatible backend pointing at the local llama-server
-- the `desktop-dev-automation-seeded-llama` recipe starts on :8899. It builds a
-- client without connecting, so deterministic runs (server down) are unaffected;
-- the LLM/HITL/A2A scripts route real inference through it. `local-qwen` (the
-- embedded runner) is kept as a non-default row: under the HOME-swap its model
-- path resolves to the seed placeholder GGUF, which does not load.
-- Three backends, chosen so the page is legible in a screenshot rather than a
-- wall of red. The list auto-pings every ENABLED backend when it opens
-- (Llm.svelte:107), and a ping can only succeed against something reachable:
--   * local-llama-server points at :8899, which the -llama recipe brings up, so
--     it is the one that shows a healthy card. It is the default.
--   * anthropic-claude stays enabled without a key, which is the honest error
--     state the "provider does not answer" troubleshooting page documents.
--   * openai-gpt4o-mini is disabled, so the list also shows a disabled card.
-- local-qwen was dropped: it needs the apollia-runner sidecar, absent in dev, so
-- it was red in every run and added nothing but a second identical error.
INSERT INTO llm_backends (name, provider, model, config_json, enabled, is_default, created_at, updated_at) VALUES
  ('local-llama-server',
   'openai',
   'local-model',
   '{"base_url":"http://127.0.0.1:8899/v1","api_key":"sk-noauth","context_size":32768,"temperature":0.2}',
   1,
   1,
   '2026-07-01T00:00:00Z',
   '2026-07-01T00:00:00Z'),
  ('openai-gpt4o-mini',
   'openai',
   'gpt-4o-mini',
   '{"base_url":"https://api.openai.com/v1","api_key":"${OPENAI_API_KEY}","context_size":128000,"temperature":0.7}',
   0,
   0,
   '2026-07-01T00:00:00Z',
   '2026-07-01T00:00:00Z'),
  ('anthropic-claude',
   'anthropic',
   'claude-3-5-sonnet-latest',
   '{"base_url":"https://api.anthropic.com","api_key":"${ANTHROPIC_API_KEY}","context_size":200000,"temperature":0.7}',
   1,
   0,
   '2026-07-01T00:00:00Z',
   '2026-07-01T00:00:00Z');

-- STT configuration singleton (id = 1). Enabled with a configured whisper model
-- path and non-default language so the settings form renders a configured state.
INSERT INTO stt_config (id, enabled, model_path, hotkey, clipboard_mode, clipboard_restore, silence_threshold_db, max_recording_sec, language, trigger_mode, updated_at) VALUES
  (1,
   1,
   '~/.apollia/models/ggml-base.bin',
   'ctrl+shift+space',
   'paste',
   1,
   -40.0,
   60,
   'en',
   'toggle',
   '2026-07-01T00:00:00Z');
