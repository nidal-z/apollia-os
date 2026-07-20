-- Deterministic seed fragment for the TRANSCRIPTIONS subsystem
-- (stt_transcriptions.db). Table: stt_transcriptions (+ _schema_version).
-- Unblocks the desktop Transcriptions page automation (stt-det.json):
--   * removes empty-state-transcriptions (list is non-empty)
--   * transcript-card-*      (list rows, Transcriptions.svelte) <- stt_transcriptions
--   * transcript-source / transcript-time / transcript-text (per-card fields)
--   * transcript-copy-* / transcript-delete-* (per-card actions)
-- Read path: list_transcriptions (desktop) -> SttRepository::list, query
--   SELECT ... FROM stt_transcriptions ORDER BY created_at DESC LIMIT ?.
-- Schema is applied separately by the builder (schemas/stt_transcriptions.sql).
--
-- Determinism notes:
--   * Fixed ids and a fixed base date 2026-07-01. The seconds field is varied
--     (04..01) ONLY so the created_at DESC ordering is stable, identical
--     timestamps would leave row order undefined. Row "alpha" (…04Z) is the
--     newest and therefore nth 0 in the list.
--   * source respects the schema CHECK (source IN ('hotkey','file','api')).

-- Schema version marker. SttRepository::open reconciles this to the current
-- version (1) with idempotent CREATE TABLE IF NOT EXISTS migrations, seeding
-- it here matches the shape of a normally-migrated database.
INSERT INTO _schema_version (version) VALUES (1);

INSERT INTO stt_transcriptions
  (id, full_text, language, source, audio_duration_ms, processing_time_ms, model_name, created_at)
VALUES
  ('seed-transcript-alpha',
   'This is the first seeded transcription. It was captured from the global hotkey and proves the transcriptions list renders a card with source, time, and text.',
   'en',
   'hotkey',
   4200,
   380,
   'ggml-base.en',
   '2026-07-01T00:00:04Z'),
  ('seed-transcript-bravo',
   'Deuxieme transcription de test issue d''un fichier audio importe. Elle demontre la variete des sources dans la liste.',
   'fr',
   'file',
   9100,
   640,
   'ggml-base',
   '2026-07-01T00:00:03Z'),
  ('seed-transcript-charlie',
   'Third seeded transcription, submitted through the local API source. Short and deterministic.',
   'en',
   'api',
   2600,
   210,
   'ggml-small.en',
   '2026-07-01T00:00:02Z'),
  ('seed-transcript-delta',
   'Fourth seeded transcription from the hotkey source, kept short so the delete and copy actions have a stable last row to target.',
   'en',
   'hotkey',
   1800,
   150,
   'ggml-base.en',
   '2026-07-01T00:00:01Z');
