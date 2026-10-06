/**
 * View-model of the "To review" screen: pure decisions over the list of
 * decisions waiting for the user. No Svelte, no i18n store, no `invoke`, so
 * every rule here is unit-testable.
 */
import type { AskUserAnswer } from "$lib/types";
import type {
  DecideRequest,
  PendingAnswer,
  PendingDecision,
} from "$lib/ipc/pendingDecisions";
import type { ConsentAnswer, ConsentDecision } from "../chat/ConsentAnswers.svelte";

/** The chips of the type filter. */
export type ReviewFilter = "all" | "decisions" | "questions" | "batch_items";

export const REVIEW_FILTERS: readonly ReviewFilter[] = [
  "all",
  "decisions",
  "questions",
  "batch_items",
];

/** How long an answered line stays, with its "Cancel", before the answer is sent. */
export const ANSWER_HOLD_MS = 6000;

/** How many lines of a command the list shows before it is unfolded. */
export const COLLAPSED_COMMAND_LINES = 4;

/** Whether an item is answered with words rather than with a consent. */
export function isQuestion(item: PendingDecision): boolean {
  return item.answers.includes("answer");
}

/**
 * The chip an item counts under: a batch item under its own chip, anything
 * answered with words under the questions, and every consent, the pause of a
 * task included, under the decisions.
 */
export function filterOf(item: PendingDecision): Exclude<ReviewFilter, "all"> {
  if (item.kind === "batch_item") return "batch_items";
  if (isQuestion(item)) return "questions";
  return "decisions";
}

function askedTime(item: PendingDecision): number {
  const time = item.askedAt ? Date.parse(item.askedAt) : Number.NaN;
  return Number.isNaN(time) ? Number.NEGATIVE_INFINITY : time;
}

/** One list, the most recent first; an item with no date goes last. */
export function newestFirst(items: readonly PendingDecision[]): PendingDecision[] {
  return [...items].sort((a, b) => askedTime(b) - askedTime(a));
}

/** How many items each chip holds. */
export function filterCounts(items: readonly PendingDecision[]): Record<ReviewFilter, number> {
  const counts: Record<ReviewFilter, number> = {
    all: items.length,
    decisions: 0,
    questions: 0,
    batch_items: 0,
  };
  for (const item of items) counts[filterOf(item)] += 1;
  return counts;
}

/** The items a chip shows. */
export function shownUnder(
  items: readonly PendingDecision[],
  filter: ReviewFilter,
): PendingDecision[] {
  return filter === "all" ? [...items] : items.filter((item) => filterOf(item) === filter);
}

/** Where an item was born, as its origin line says it. */
export type ReviewOrigin =
  | { kind: "conversation"; sessionId: string }
  | { kind: "task"; taskId: string }
  | { kind: "batch"; batchId: string; position: number }
  | { kind: "unknown" };

export function originOf(item: PendingDecision): ReviewOrigin {
  const { origin } = item;
  if (item.kind === "batch_item" && origin.batchId) {
    return { kind: "batch", batchId: origin.batchId, position: (origin.rank ?? 0) + 1 };
  }
  if (item.kind === "task_pause" && origin.taskId) {
    return { kind: "task", taskId: origin.taskId };
  }
  if (origin.sessionId) return { kind: "conversation", sessionId: origin.sessionId };
  if (origin.taskId) return { kind: "task", taskId: origin.taskId };
  return { kind: "unknown" };
}

const CARD_ANSWERS: readonly ConsentAnswer[] = ["once", "always", "refuse"];

/** The answers the decision card offers for an item, in the card's order. */
export function cardAnswers(item: PendingDecision): ConsentAnswer[] {
  return CARD_ANSWERS.filter((answer) => item.answers.includes(answer));
}

/** The request that carries an answer of the decision card. */
export function consentRequest(item: PendingDecision, decision: ConsentDecision): DecideRequest {
  if (decision.decision === "refuse") {
    return { id: item.id, answer: "refuse", reason: decision.reason };
  }
  const answer: PendingAnswer = decision.decision === "always_accept" ? "always" : "once";
  return { id: item.id, answer };
}

/** A command as the list shows it before it is unfolded. */
export interface CollapsedCommand {
  /** The lines shown. */
  text: string;
  shown: number;
  total: number;
  /** Lines are left out: the answers wait until the whole command is shown. */
  collapsed: boolean;
}

/** The first lines of `code`, and whether any is left out. */
export function collapsedCommand(
  code: string,
  maxLines: number = COLLAPSED_COMMAND_LINES,
): CollapsedCommand {
  const lines = code.split("\n");
  if (lines.length <= maxLines) {
    return { text: code, shown: lines.length, total: lines.length, collapsed: false };
  }
  return {
    text: lines.slice(0, maxLines).join("\n"),
    shown: maxLines,
    total: lines.length,
    collapsed: true,
  };
}

/** One question as the question card draws it. */
export interface QuestionField {
  id: string;
  text: string;
  /** `open` takes words, `single` one choice, `multi` several, `confirm` yes or no. */
  kind: "open" | "single" | "multi" | "confirm";
  options: { value: string; label: string }[];
  hint: string | null;
  /** The answer is a number when it reads as one (a bound). */
  numeric: boolean;
}

function record(value: unknown): Record<string, unknown> | null {
  return typeof value === "object" && value !== null && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

function text(value: unknown): string {
  return typeof value === "string" ? value : "";
}

function askUserField(raw: unknown, index: number): QuestionField | null {
  const q = record(raw);
  if (!q) return null;
  const options = Array.isArray(q.options)
    ? q.options.filter((o): o is string => typeof o === "string")
    : [];
  const type = text(q.type);
  const kind = type === "multi_choice" ? "multi" : type === "single_choice" ? "single" : "open";
  return {
    id: text(q.id) || String(index),
    text: text(q.question),
    kind: options.length === 0 ? "open" : kind,
    options: options.map((o) => ({ value: o, label: o })),
    hint: text(q.hint) || null,
    numeric: false,
  };
}

function pauseField(payload: Record<string, unknown>): QuestionField {
  const propositions = Array.isArray(payload.propositions) ? payload.propositions : [];
  const options = propositions
    .map(record)
    .filter((p): p is Record<string, unknown> => p !== null)
    .map((p) => ({ value: text(p.id), label: text(p.libelle) || text(p.id) }));
  const genre = text(payload.genre);
  const kind = options.length > 0 ? "single" : genre === "confirmation" ? "confirm" : "open";
  return {
    id: "answer",
    text: text(payload.question),
    kind,
    options,
    hint: null,
    numeric: genre === "seuil",
  };
}

/**
 * The questions of an item answered with words: the questions an agent asked
 * the user, or the one typed question of a paused task.
 */
export function questionFields(item: PendingDecision): QuestionField[] {
  if (item.kind === "task_pause") {
    const payload = record(item.questions);
    return payload ? [pauseField(payload)] : [];
  }
  if (!Array.isArray(item.questions)) return [];
  return item.questions
    .map(askUserField)
    .filter((field): field is QuestionField => field !== null);
}

/** What the user typed and picked, keyed by question id. */
export interface QuestionDraft {
  picked: Record<string, string[]>;
  /** The free answer: the whole answer of an open question, "other" beside choices. */
  other: Record<string, string>;
}

export function emptyDraft(): QuestionDraft {
  return { picked: {}, other: {} };
}

function otherOf(draft: QuestionDraft, id: string): string {
  return (draft.other[id] ?? "").trim();
}

function fieldAnswered(field: QuestionField, draft: QuestionDraft): boolean {
  return otherOf(draft, field.id).length > 0 || (draft.picked[field.id] ?? []).length > 0;
}

/** At least one question carries an answer, so there is something to send. */
export function canAnswer(fields: readonly QuestionField[], draft: QuestionDraft): boolean {
  return fields.some((field) => fieldAnswered(field, draft));
}

function askUserAnswer(field: QuestionField, draft: QuestionDraft): AskUserAnswer {
  const other = otherOf(draft, field.id);
  const picked = draft.picked[field.id] ?? [];
  if (field.kind === "multi") {
    const values = other.length > 0 ? [...picked, other] : [...picked];
    return { id: field.id, value: null, values, skipped: values.length === 0 };
  }
  // A typed answer wins over a picked choice: it is the more specific of the two.
  const value = other.length > 0 ? other : (picked[0] ?? null);
  return { id: field.id, value, values: [], skipped: value === null };
}

function pauseValue(field: QuestionField, draft: QuestionDraft): string | number | boolean {
  const other = otherOf(draft, field.id);
  const picked = (draft.picked[field.id] ?? [])[0];
  if (other.length === 0 && picked !== undefined) {
    if (field.kind === "confirm") return picked === "true";
    return picked;
  }
  if (field.numeric) {
    const number = Number(other.replace(",", "."));
    if (other.length > 0 && Number.isFinite(number)) return number;
  }
  return other;
}

/** The request that carries the answers typed in the question card. */
export function questionRequest(
  item: PendingDecision,
  fields: readonly QuestionField[],
  draft: QuestionDraft,
): DecideRequest {
  if (item.kind === "task_pause") {
    const field = fields[0];
    return {
      id: item.id,
      answer: "answer",
      value: field ? pauseValue(field, draft) : "",
    };
  }
  return {
    id: item.id,
    answer: "answer",
    answers: fields.map((field) => askUserAnswer(field, draft)),
  };
}

/** The request that declines an item, with the reason when one was typed. */
export function refusalRequest(item: PendingDecision, reason: string): DecideRequest {
  const trimmed = reason.trim();
  return { id: item.id, answer: "refuse", reason: trimmed.length > 0 ? trimmed : null };
}
