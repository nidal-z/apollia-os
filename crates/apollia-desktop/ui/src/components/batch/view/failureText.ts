// The failure of a batch item, or of one of its steps, said in plain words.
//
// The runtime reports a failure as it is raised (`internal error:
// REPLAN_FAILED: plan parse/validation failed after 3 attempts: ...`). The
// screen of a batch says what happened and what was tried; the raw text stays
// at hand, folded, for whoever needs it.

/** A catalogue lookup, as `$t` answers it. */
type Translate = (key: string, options?: { values: Record<string, string | number> }) => string;

/** A failure in plain words, and the raw text it was read from. */
export interface FailureText {
  text: string;
  /** Where a file of the name looked for is, when the runtime found one. */
  hint: string | null;
  raw: string;
}

/** The failure `raw` said in plain words. */
export function failureText(raw: string, tr: Translate): FailureText {
  const hintMatch = /A file of that name is in the folders granted to this agent: (.+)$/m.exec(raw);
  const hint = hintMatch ? tr("batch_view.failure.hint", { values: { paths: hintMatch[1].trim() } }) : null;
  const missing = /not found: '([^']+)'/.exec(raw);
  if (missing) return { text: tr("batch_view.failure.not_found", { values: { path: missing[1] } }), hint, raw };
  const tool = /tool '([^']+)' is not granted/.exec(raw);
  if (tool) return { text: tr("batch_view.failure.tool", { values: { tool: tool[1] } }), hint, raw };
  const outside = /path '([^']+)' is outside every folder granted/.exec(raw);
  if (outside) return { text: tr("batch_view.failure.outside", { values: { path: outside[1] } }), hint, raw };
  const deadline = /ran past (\d+) minutes/.exec(raw);
  if (deadline) return { text: tr("batch_view.failure.deadline", { values: { minutes: deadline[1] } }), hint, raw };
  if (raw.includes("REPLAN_FAILED")) return { text: tr("batch_view.failure.replan"), hint, raw };
  if (raw.includes("PLAN_FAILED")) return { text: tr("batch_view.failure.plan"), hint, raw };
  if (/budget/i.test(raw)) return { text: tr("batch_view.failure.budget"), hint, raw };
  return { text: raw, hint, raw };
}
