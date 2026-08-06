-- Deterministic seed fragment for the AUDIT subsystem (audit.db).
-- Table: tool_invocations. Schema applied separately by the builder.
--
-- Unblocks two screens that were shipping their empty state into the
-- documentation:
--   * Observability > Audit Trail: audit-stats (4 KPIs), audit-trail-table and
--     the expandable audit-row-* which reveals args_json / stdout / stderr.
--   * Observability > Timeline: scan_audit_db turns each row into a "tool"
--     event, so the timeline has something inside its default 1 h window.
--
-- Timestamps are relative, and that is the point rather than a detail. The rest
-- of the seed hardcodes 2026-07-01, which was already thirty days old when this
-- was written and drifts one day further every day: those rows fall outside
-- every timeline window the interface offers, so the screen looks broken for a
-- reason nobody would guess.
--
-- They use strftime with the explicit ISO form rather than datetime(), and that
-- distinction cost the first replay a screen. The timeline compares started_at
-- against a cutoff formatted '%Y-%m-%dT%H:%M:%SZ', lexicographically.
-- datetime() yields '2026-07-31 12:43:28', whose space sorts below 'T', so
-- every row was excluded and the timeline stayed empty, while the audit page,
-- which parses rather than compares, showed the same rows perfectly.
--
-- Coverage is chosen so the filters have something to bite on:
--   * three distinct tools, so the Tool filter is not a single-entry list,
--   * two agents, so the Agent filter is meaningful,
--   * one failure with a non-zero exit code and stderr, so the Failures KPI is
--     not zero and the red badge renders,
--   * durations spread wide enough that the average is not a round number.

INSERT INTO tool_invocations
  (id, agent_id, task_id, tool_name, input_hash, sandbox_profile,
   started_at, duration_ms, exit_code, success, error_code, resources_used,
   args_json, stdout, stderr, run_id)
VALUES
  ('seed-audit-1', 'seed-classifier', 'seed-task-done-1', 'file_read',
   'sha256:1f0a', 'restricted',
   strftime('%Y-%m-%dT%H:%M:%SZ', 'now', '-4 minutes'), 34, 0, 1, NULL, '{"cpu_ms":12,"peak_rss_kb":8100}',
   '{"path":"notes/roadmap.md"}',
   'Q3 roadmap draft, 42 lines read.', NULL, 'seed-run-1'),

  ('seed-audit-2', 'seed-classifier', 'seed-task-done-1', 'web_search',
   'sha256:2b71', 'restricted',
   strftime('%Y-%m-%dT%H:%M:%SZ', 'now', '-11 minutes'), 812, 0, 1, NULL, '{"cpu_ms":56,"peak_rss_kb":21400}',
   '{"query":"sovereign runtime for local agents","limit":5}',
   '5 results returned, top domain: openssf.org', NULL, 'seed-run-1'),

  ('seed-audit-3', 'apollia-guide', 'seed-task-done-2', 'file_write',
   'sha256:3c92', 'restricted',
   strftime('%Y-%m-%dT%H:%M:%SZ', 'now', '-26 minutes'), 47, 0, 1, NULL, '{"cpu_ms":9,"peak_rss_kb":7600}',
   '{"path":"output/summary.txt","bytes":1284}',
   'Wrote 1284 bytes.', NULL, 'seed-run-2'),

  ('seed-audit-4', 'apollia-guide', 'seed-task-error-1', 'bash_executor',
   'sha256:4d13', 'restricted',
   strftime('%Y-%m-%dT%H:%M:%SZ', 'now', '-38 minutes'), 1503, 2, 0, 'ExitNonZero', '{"cpu_ms":140,"peak_rss_kb":33200}',
   '{"command":"grep -r TODO src/"}',
   NULL, 'grep: src/: No such file or directory', 'seed-run-3'),

  ('seed-audit-5', 'seed-classifier', 'seed-task-run-1', 'file_read',
   'sha256:5e44', 'restricted',
   strftime('%Y-%m-%dT%H:%M:%SZ', 'now', '-52 minutes'), 28, 0, 1, NULL, '{"cpu_ms":11,"peak_rss_kb":8000}',
   '{"path":"notes/meeting-2026-07.md"}',
   'Meeting notes, 18 lines read.', NULL, 'seed-run-4'),

  ('seed-audit-6', 'apollia-guide', 'seed-task-done-2', 'web_search',
   'sha256:6f55', 'restricted',
   strftime('%Y-%m-%dT%H:%M:%SZ', 'now', '-3 hours'), 1140, 0, 1, NULL, '{"cpu_ms":74,"peak_rss_kb":22800}',
   '{"query":"MCP transport specification","limit":3}',
   '3 results returned, top domain: modelcontextprotocol.io', NULL, 'seed-run-2');
