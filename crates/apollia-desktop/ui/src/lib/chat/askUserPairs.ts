/**
 * The questions an agent asked and the answers it got, paired for reading.
 *
 * The question tool keeps its questions in `input.questions` and the answers
 * in its output as `{ answers: [{ id, value?, values?, skipped }] }`.
 */

/** One question with its answer, or with nothing yet. */
export interface AskUserPair {
  id: string;
  question: string;
  answer: string;
  skipped: boolean;
}

interface Question {
  id: string;
  question: string;
}

interface Answer {
  id: string;
  value?: string | null;
  values?: string[];
  skipped: boolean;
}

/** Pair questions and answers; `null` when there is neither, or no parse. */
export function askUserPairs(
  args: Record<string, unknown>,
  output: string | null,
): AskUserPair[] | null {
  const raw = (args as { questions?: unknown }).questions;
  const questions = Array.isArray(raw) ? (raw as Question[]) : [];
  let answers: Answer[] = [];
  if (typeof output === "string" && output.length > 0) {
    try {
      const parsed = JSON.parse(output) as { answers?: Answer[] };
      if (Array.isArray(parsed?.answers)) answers = parsed.answers;
    } catch {
      return null;
    }
  }
  if (questions.length === 0 && answers.length === 0) return null;
  const byId = new Map(questions.map((q) => [q.id, q]));
  if (answers.length === 0) {
    return questions.map((q) => ({ id: q.id, question: q.question, answer: "", skipped: false }));
  }
  return answers.map((a) => ({
    id: a.id,
    question: byId.get(a.id)?.question ?? a.id,
    answer: a.skipped ? "" : a.values && a.values.length > 0 ? a.values.join(", ") : (a.value ?? ""),
    skipped: a.skipped,
  }));
}
