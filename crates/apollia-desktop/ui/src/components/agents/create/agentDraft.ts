/**
 * The draft of an agent, between the form that asks for it and the form the
 * person reads it over in.
 *
 * "Create an agent" opens a short form. The agent workshop, a workflow shipped
 * with Apollia, takes the form with what the machine holds, asks the person
 * what the form leaves unsaid, and hands a sheet back. The runtime checks
 * that sheet, and the draft it answers opens in the form of an agent sheet.
 * These helpers build what the workshop is launched on, read what it hands
 * back, and say what a save sends. Kept out of the Svelte components so the
 * decisions run under Vitest (node environment). Nothing here calls the
 * runtime.
 */
import type { AgentSheetDraft, DraftOffer, ProposedSheet } from "$lib/ipc/agentCreate";
import type { ResolvedAgent } from "$lib/ipc/agentSheet";
import type { PendingDecision } from "$lib/ipc/pendingDecisions";
import type { WorkflowStep } from "$lib/ipc/workflows";
import {
  chooseModel,
  cleanExamples,
  type AdjustDraft,
  type ModelDraft,
  type ModelOption,
} from "../sheet/agentAdjust";

/** The workflow of the agent workshop, as `<package>/<workflow>`. */
export const WORKSHOP_WORKFLOW = "agent-builder/draft";

/** One access of a draft. */
export interface ProposedAccess {
  id: string;
  /**
   * Whether the form opens with it granted. A sensitive access is only when
   * the default access of Settings > Integrations grants it to every new agent.
   */
  checked: boolean;
  sensitive: boolean;
  /** Added because an agent created without code is granted it by default. */
  by_default: boolean;
}

/** The draft of an agent, as the runtime answers it once a sheet is checked. */
export interface ProposedAgent {
  name: string;
  mission: string;
  examples: string[];
  /** The model proposed; `null` follows the default model. */
  model: string | null;
  access: ProposedAccess[];
  sources: string[];
  /** What was asked for and the machine does not hold. */
  unavailable: string[];
}

// ── The form the workshop is launched on ─────────────────────────────────

/** The form of the creation, as the person fills it. */
export interface WorkshopForm {
  /** What the agent is for; the one field a launch needs. */
  purpose: string;
  /** What it must give back, and in what form. */
  result: string;
  /** What it must always or never do. */
  rules: string;
  /** Requests it will receive, one per line. */
  examples: string;
  /** The sources checked. */
  sources: string[];
  /** The action families checked beyond plain reading. */
  rights: string[];
}

/** An empty form. */
export function emptyForm(): WorkshopForm {
  return { purpose: "", result: "", rules: "", examples: "", sources: [], rights: [] };
}

/** Whether the form holds what a launch needs: what the agent is for. */
export function canPrepare(form: WorkshopForm): boolean {
  return form.purpose.trim().length > 0;
}

/**
 * The action families the form offers beyond plain reading, in a fixed
 * order: only those a tool of the machine belongs to, so a box never grants
 * nothing.
 */
export const RIGHT_FAMILIES = ["read_personal", "write_or_delete", "send_external", "execute_code"] as const;
export type RightFamily = (typeof RIGHT_FAMILIES)[number];

export function offeredRights(offer: DraftOffer | null): RightFamily[] {
  const held = new Set((offer?.accesses ?? []).map((a) => a.family));
  return RIGHT_FAMILIES.filter((family) => held.has(family));
}

/**
 * The inputs the workshop is launched on: the fields the person filled, the
 * empty ones left out, the language of the interface, and what the machine
 * holds, as JSON. A source or a right the machine no longer offers is left
 * out.
 */
export function workshopInputs(
  form: WorkshopForm,
  offer: DraftOffer | null,
  locale: string | null | undefined,
): Record<string, unknown> {
  const inputs: Record<string, unknown> = { purpose: form.purpose.trim() };
  for (const field of ["result", "rules", "examples"] as const) {
    const text = form[field].trim();
    if (text) inputs[field] = text;
  }
  const held = new Set((offer?.sources ?? []).map((s) => s.id));
  const sources = form.sources.filter((id) => held.has(id));
  if (sources.length > 0) inputs.sources = sources;
  const offered: readonly string[] = offeredRights(offer);
  const rights = form.rights.filter((family) => offered.includes(family));
  if (rights.length > 0) inputs.rights = rights;
  inputs.language = (locale ?? "").toLowerCase().startsWith("fr") ? "fr" : "en";
  if (offer) inputs.offer = JSON.stringify({ accesses: offer.accesses, sources: offer.sources });
  return inputs;
}

// ── What the workshop hands back ─────────────────────────────────────────

function strings(value: unknown): string[] {
  return Array.isArray(value) ? value.filter((v): v is string => typeof v === "string") : [];
}

/**
 * The sheet a finished run hands back, as the runtime is asked to check it;
 * `null` when the output is not a sheet with a name and a mission.
 */
export function sheetOfRun(output: unknown): ProposedSheet | null {
  let read = output;
  if (typeof read === "string") {
    try {
      read = JSON.parse(read);
    } catch {
      return null;
    }
  }
  if (typeof read !== "object" || read === null || Array.isArray(read)) return null;
  const raw = read as Record<string, unknown>;
  if (typeof raw.name !== "string" || typeof raw.mission !== "string") return null;
  if (raw.name.trim().length === 0 || raw.mission.trim().length === 0) return null;
  return {
    name: raw.name,
    mission: raw.mission,
    examples: strings(raw.examples),
    access: strings(raw.access),
    sources: strings(raw.sources),
  };
}

function accessOf(value: unknown): ProposedAccess[] {
  if (!Array.isArray(value)) return [];
  const out: ProposedAccess[] = [];
  for (const entry of value) {
    if (typeof entry !== "object" || entry === null) continue;
    const raw = entry as Record<string, unknown>;
    if (typeof raw.id !== "string" || raw.id.length === 0) continue;
    const sensitive = raw.sensitive === true;
    const byDefault = raw.by_default === true;
    out.push({
      id: raw.id,
      // A sensitive access reads as checked only when the default access
      // grants it: the runtime sets that mark, never the sheet.
      checked: raw.checked === true && (!sensitive || byDefault),
      sensitive,
      by_default: byDefault,
    });
  }
  return out;
}

/** The draft the runtime answers for a checked sheet; `null` when it is not one. */
export function draftOfChecked(checked: unknown): ProposedAgent | null {
  if (typeof checked !== "object" || checked === null || Array.isArray(checked)) return null;
  const draft = checked as Record<string, unknown>;
  if (typeof draft.name !== "string" || typeof draft.mission !== "string") return null;
  return {
    name: draft.name,
    mission: draft.mission,
    examples: strings(draft.examples),
    model: typeof draft.model === "string" && draft.model.length > 0 ? draft.model : null,
    access: accessOf(draft.access),
    sources: strings(draft.sources),
    unavailable: strings(draft.unavailable),
  };
}

// ── The run, while it goes on ────────────────────────────────────────────

/** The steps of the workshop, in the order they run. */
export const WORKSHOP_STEPS = ["understand", "ask", "write"] as const;
export type WorkshopStep = (typeof WORKSHOP_STEPS)[number];

/** Where a step of the run stands, as the progress shows it. */
export type StepState = "done" | "current" | "upcoming" | "failed";

/** One step of the run, as shown while the workshop works. */
export interface StepView {
  step: WorkshopStep;
  state: StepState;
}

/**
 * The three steps, each done, in progress, still to come or failed. Before
 * the run reports anything, the first one is in progress.
 */
export function stepViews(steps: WorkflowStep[]): StepView[] {
  const byName = new Map(steps.map((s) => [s.name, s.status]));
  let currentGiven = false;
  return WORKSHOP_STEPS.map((step) => {
    const status = byName.get(step);
    if (status === "completed") return { step, state: "done" };
    if (status === "failed" || status === "canceled") return { step, state: "failed" };
    if (!currentGiven) {
      currentGiven = true;
      return { step, state: "current" };
    }
    return { step, state: "upcoming" };
  });
}

/** The question the run waits on, as the creation shows it. */
export interface WorkshopQuestion {
  /** Identifier the answer is sent by. */
  id: string;
  question: string;
  /** The proposed answers; empty for a free answer only. */
  options: string[];
}

/**
 * The question the run `runId` waits on, among the decisions waiting for the
 * user; `null` when it waits on none.
 */
export function questionOf(decisions: PendingDecision[], runId: string): WorkshopQuestion | null {
  const waiting = decisions.find((d) => d.kind === "question" && d.origin.runId === runId);
  if (!waiting) return null;
  const first = Array.isArray(waiting.questions) ? (waiting.questions[0] as unknown) : null;
  const raw = typeof first === "object" && first !== null ? (first as Record<string, unknown>) : {};
  return {
    id: waiting.id,
    question: typeof raw.question === "string" && raw.question.trim() ? raw.question : waiting.summary,
    options: strings(raw.options),
  };
}

// ── The draft in the form ────────────────────────────────────────────────

/** Why an access is listed in the form, beyond having been asked for. */
export type AccessNote = "by_default" | "sensitive";

/** The draft as the form of a sheet opens it. Nothing of it is stored. */
export interface DraftReview {
  /** Display name, edited in the form. */
  name: string;
  /** Mission, examples, model, the accesses checked and the sources checked. */
  form: AdjustDraft;
  /** Every access the form lists, checked or not, in the order of the draft. */
  listed: string[];
  /** The accesses checked when the form opens. */
  checked: string[];
  /** The sources checked when the form opens. */
  sources: string[];
  /** The accesses that carry a note. */
  notes: Record<string, AccessNote>;
  /** What was asked for and the machine does not hold. */
  unavailable: string[];
}

/** The model the draft names, when the machine offers it; `null` otherwise. */
function modelOf(name: string | null, options: ModelOption[]): ModelDraft | null {
  if (!name || !options.some((o) => o.id === name)) return null;
  return chooseModel(options, null, name);
}

/**
 * The form a draft opens: its accesses listed, the sensitive ones unchecked
 * unless the default access grants them, its model kept only when the machine
 * offers it. `null` opens an empty
 * form, for a sheet written by hand.
 */
export function reviewOf(draft: ProposedAgent | null, options: ModelOption[]): DraftReview {
  const access = draft?.access ?? [];
  const checked = access
    .filter((a) => a.checked && (!a.sensitive || a.by_default))
    .map((a) => a.id);
  const notes: Record<string, AccessNote> = {};
  for (const a of access) {
    if (a.sensitive) notes[a.id] = "sensitive";
    else if (a.by_default) notes[a.id] = "by_default";
  }
  const sources = [...new Set(draft?.sources ?? [])];
  return {
    name: draft?.name.trim() ?? "",
    form: {
      mission: draft?.mission.trim() ?? "",
      examples: cleanExamples(draft?.examples ?? []),
      access: [...checked],
      sources: [...sources],
      model: modelOf(draft?.model ?? null, options),
    },
    listed: [...new Set(access.map((a) => a.id))],
    checked,
    sources,
    notes,
    unavailable: [...(draft?.unavailable ?? [])],
  };
}

/** A line of `unavailable`, sorted by what it names. */
export interface UnavailableView {
  kind: "access" | "source" | "model";
  name: string;
}

/** What the machine does not hold, each entry with what it is. */
export function unavailableViews(unavailable: string[]): UnavailableView[] {
  return unavailable.map((entry) => {
    if (entry.startsWith("source:")) return { kind: "source", name: entry.slice("source:".length) };
    if (entry.startsWith("model:")) return { kind: "model", name: entry.slice("model:".length) };
    return { kind: "access", name: entry };
  });
}

// ── Saving ───────────────────────────────────────────────────────────────

/** Whether the form holds what an agent needs: a name and a mission. */
export function canSave(name: string, form: AdjustDraft): boolean {
  return name.trim().length > 0 && form.mission.trim().length > 0;
}

/** The sheet a save sends: trimmed, the empty examples dropped, no duplicate. */
export function sheetOf(name: string, form: AdjustDraft): AgentSheetDraft {
  return {
    name: name.trim(),
    mission: form.mission.trim(),
    examples: cleanExamples(form.examples),
    access: [...new Set(form.access)],
    sources: [...new Set(form.sources)],
  };
}

/** What a save reaches. */
export interface SaveEffects {
  create: (sheet: AgentSheetDraft) => Promise<ResolvedAgent>;
  setModel: (agent: string, model: ModelDraft) => Promise<void>;
}

/** What a save did. */
export interface SaveOutcome {
  agent: ResolvedAgent;
  /** The agent exists but its model could not be assigned; the reason. */
  modelError: string | null;
}

/**
 * Save the form: the only moment anything is written. Creates the agent,
 * then assigns its model when one was chosen. A form without a name or a
 * mission reaches no effect and answers `null`. A model that cannot be
 * assigned leaves the agent created, on the default model, and says why.
 */
export async function saveDraft(
  name: string,
  form: AdjustDraft,
  effects: SaveEffects,
): Promise<SaveOutcome | null> {
  if (!canSave(name, form)) return null;
  const agent = await effects.create(sheetOf(name, form));
  if (!form.model) return { agent, modelError: null };
  try {
    await effects.setModel(agent.id, form.model);
    return { agent, modelError: null };
  } catch (err) {
    return { agent, modelError: err instanceof Error ? err.message : String(err) };
  }
}

// ── Catalogue keys ───────────────────────────────────────────────────────

/** Label of each right the form offers. */
export const RIGHT_LABEL: Record<RightFamily, string> = {
  read_personal: "agent_create.right_read_personal",
  write_or_delete: "agent_create.right_write",
  send_external: "agent_create.right_send",
  execute_code: "agent_create.right_execute",
};

/** Label of each step of the workshop. */
export const STEP_LABEL: Record<WorkshopStep, string> = {
  understand: "agent_create.step_understand",
  ask: "agent_create.step_ask",
  write: "agent_create.step_write",
};

/** Badge of each note an access carries in the form. */
export const ACCESS_NOTE_LABEL: Record<AccessNote, string> = {
  by_default: "agent_create.access_by_default",
  sensitive: "agent_create.access_sensitive",
};

/** Sentence of each kind of entry the machine does not hold. */
export const UNAVAILABLE_LABEL: Record<UnavailableView["kind"], string> = {
  access: "agent_create.unavailable_access",
  source: "agent_create.unavailable_source",
  model: "agent_create.unavailable_model",
};
