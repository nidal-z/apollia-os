/**
 * Pure helper for `AskUserForm.svelte` - converts per-question UI state into
 * the structured `AskUserAnswer[]` payload the runtime expects.
 *
 * Kept out of the Svelte component so unit tests can exercise the validation
 * logic without spinning up a DOM.
 */
import type { AskUserAnswer, AskUserQuestion } from "$lib/types";

/** Snapshot of the form state, keyed by `question.id`. */
export interface AskUserFormState {
  open: Record<string, string>;
  single: Record<string, string>;
  multi: Record<string, Record<string, boolean>>;
}

/**
 * Build the answers array from the form state.
 *
 * Validation is soft: an empty open input or an unselected radio/checkbox
 * group becomes `{ skipped: true }` rather than blocking submission.
 */
export function buildAskUserAnswers(
  questions: AskUserQuestion[],
  state: AskUserFormState,
): AskUserAnswer[] {
  return questions.map((q) => {
    if (q.type === "open") {
      const v = (state.open[q.id] ?? "").trim();
      return v.length === 0
        ? { id: q.id, skipped: true }
        : { id: q.id, value: v, skipped: false };
    }
    if (q.type === "single_choice") {
      const v = state.single[q.id];
      return v == null || v.length === 0
        ? { id: q.id, skipped: true }
        : { id: q.id, value: v, skipped: false };
    }
    // multi_choice
    const selected = Object.entries(state.multi[q.id] ?? {})
      .filter(([, on]) => on)
      .map(([opt]) => opt);
    return selected.length === 0
      ? { id: q.id, skipped: true }
      : { id: q.id, values: selected, skipped: false };
  });
}
