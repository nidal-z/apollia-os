/**
 * The creation of an agent under way, kept apart from the screen that shows
 * it.
 *
 * A creation takes minutes: a form, a workshop that asks questions and
 * writes a sheet, a review. Leaving the screen, or the application, used to
 * lose all of it, and stopped the workshop. The creation is now a draft held
 * here and kept in `localStorage`: the form typed, the run of the workshop
 * still going, the sheet under review with its edits. Coming back opens on
 * it, the workshop is followed again where it is, and only "Discard the
 * draft" or the save ends it.
 *
 * Nothing of it is an agent: the draft is never sent to the runtime, which
 * writes an agent at the save only.
 */
import { get, writable, type Writable } from "svelte/store";
import type { AdjustDraft } from "../sheet/agentAdjust";
import type { AccessNote, UnavailableView, WorkshopForm } from "./agentDraft";

/** Where a creation is held: the Agents screen, or the first launch. */
export type CreationScope = "agents" | "onboarding";

export type CreationPhase = "form" | "drafting" | "review";

/** The sheet under review, with the edits made in its form. */
export interface CreationReview {
  name: string;
  form: AdjustDraft;
  listed: string[];
  checked: string[];
  notes: Record<string, AccessNote>;
  missing: UnavailableView[];
  /** The draft of the workshop the form was filled from. */
  key: string | null;
}

export interface CreationDraft {
  phase: CreationPhase;
  wish: WorkshopForm;
  /** The run of the workshop, while it goes on. */
  runId: string | null;
  review: CreationReview | null;
  /** When the draft last changed, ISO 8601. */
  updatedAt: string;
}

export const CREATION_STORAGE_KEY = "apollia-agent-creation";

/** The catalogue key of the line that says where a creation stands. */
export const PHASE_LABEL: Record<CreationPhase, string> = {
  form: "agents.creation_draft.form",
  drafting: "agents.creation_draft.drafting",
  review: "agents.creation_draft.review",
};

function storageKey(scope: CreationScope): string {
  return `${CREATION_STORAGE_KEY}:${scope}`;
}

/** `true` when the form holds anything typed or checked. */
export function wishStarted(wish: WorkshopForm): boolean {
  return (
    [wish.purpose, wish.result, wish.rules, wish.examples].some((text) => text.trim() !== "") ||
    wish.sources.length > 0 ||
    wish.rights.length > 0
  );
}

/** `true` when the draft is worth keeping: something typed, a run, a sheet. */
export function draftStarted(draft: CreationDraft | null): draft is CreationDraft {
  return draft !== null && (draft.runId !== null || draft.review !== null || wishStarted(draft.wish));
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function strings(value: unknown): string[] | null {
  return Array.isArray(value) && value.every((entry) => typeof entry === "string") ? value : null;
}

function parseWish(value: unknown): WorkshopForm | null {
  if (!isRecord(value)) return null;
  const { purpose, result, rules, examples } = value;
  const sources = strings(value.sources);
  const rights = strings(value.rights);
  if (
    typeof purpose !== "string" ||
    typeof result !== "string" ||
    typeof rules !== "string" ||
    typeof examples !== "string" ||
    sources === null ||
    rights === null
  ) {
    return null;
  }
  return { purpose, result, rules, examples, sources, rights };
}

function parseReview(value: unknown): CreationReview | null {
  if (!isRecord(value) || typeof value.name !== "string" || !isRecord(value.form)) return null;
  const listed = strings(value.listed);
  const checked = strings(value.checked);
  if (listed === null || checked === null || !isRecord(value.notes) || !Array.isArray(value.missing)) return null;
  const form = value.form;
  if (typeof form.mission !== "string" || strings(form.access) === null || strings(form.sources) === null) return null;
  return {
    name: value.name,
    form: form as unknown as AdjustDraft,
    listed,
    checked,
    notes: value.notes as Record<string, AccessNote>,
    missing: value.missing as UnavailableView[],
    key: typeof value.key === "string" ? value.key : null,
  };
}

/** A stored draft, or none when it is missing, unreadable or of another shape. */
export function parseCreationDraft(raw: string | null): CreationDraft | null {
  if (!raw) return null;
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch {
    return null;
  }
  if (!isRecord(parsed)) return null;
  const wish = parseWish(parsed.wish);
  const phase = parsed.phase;
  if (wish === null || (phase !== "form" && phase !== "drafting" && phase !== "review")) return null;
  const review = parsed.review === null || parsed.review === undefined ? null : parseReview(parsed.review);
  const runId = typeof parsed.runId === "string" ? parsed.runId : null;
  const draft: CreationDraft = {
    // A phase whose content did not survive opens on the form.
    phase: (phase === "review" && review === null) || (phase === "drafting" && runId === null) ? "form" : phase,
    wish,
    runId,
    review,
    updatedAt: typeof parsed.updatedAt === "string" ? parsed.updatedAt : new Date(0).toISOString(),
  };
  return draftStarted(draft) ? draft : null;
}

function read(scope: CreationScope): CreationDraft | null {
  if (typeof localStorage === "undefined") return null;
  try {
    return parseCreationDraft(localStorage.getItem(storageKey(scope)));
  } catch {
    return null;
  }
}

function write(scope: CreationScope, draft: CreationDraft | null): void {
  if (typeof localStorage === "undefined") return;
  try {
    if (draftStarted(draft)) localStorage.setItem(storageKey(scope), JSON.stringify(draft));
    else localStorage.removeItem(storageKey(scope));
  } catch {
    /* quota or privacy mode: the draft lives for this session only */
  }
}

const stores = new Map<CreationScope, Writable<CreationDraft | null>>();

/** The creation under way in `scope`, `null` when there is none. */
export function creationDraft(scope: CreationScope): Writable<CreationDraft | null> {
  let store = stores.get(scope);
  if (!store) {
    store = writable<CreationDraft | null>(read(scope));
    store.subscribe((draft) => write(scope, draft));
    stores.set(scope, store);
  }
  return store;
}

/** Keeps `draft` as the creation of `scope`; one with nothing in it is dropped. */
export function keepCreation(scope: CreationScope, draft: CreationDraft | null): void {
  creationDraft(scope).set(draftStarted(draft) ? draft : null);
}

/** The creation of `scope` as it stands now. */
export function currentCreation(scope: CreationScope): CreationDraft | null {
  return get(creationDraft(scope));
}

/** Ends the creation of `scope`: saved, or discarded. */
export function endCreation(scope: CreationScope): void {
  creationDraft(scope).set(null);
}
