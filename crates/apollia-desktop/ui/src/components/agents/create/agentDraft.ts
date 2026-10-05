/**
 * The draft of an agent, between the conversation that writes it and the
 * form the person reads it over in.
 *
 * In the conversation "Create an agent" opens, the Apollia agent calls its
 * tool `propose_agent`, which answers with a draft and stores nothing. The
 * draft stays in the conversation as the output of that call. These helpers
 * read it there, lay it over the form of an agent sheet, and say what a save
 * sends. Kept out of the Svelte components so the decisions run under Vitest
 * (node environment). Nothing here calls the runtime.
 */
import type { AgentSheetDraft } from "$lib/ipc/agentCreate";
import type { ResolvedAgent } from "$lib/ipc/agentSheet";
import type { ChatMessageView, CreateSessionRequest } from "$lib/types";
import {
  chooseModel,
  cleanExamples,
  type AdjustDraft,
  type ModelDraft,
  type ModelOption,
} from "../sheet/agentAdjust";

/** The tool of the Apollia agent that drafts a sheet, as the runtime names it. */
export const AGENT_DRAFT_TOOL = "propose_agent";

/**
 * The conversation that drafts an agent: a conversation with Apollia whose
 * tool list names the draft tool, which is how the runtime recognises it.
 */
export const DRAFT_SESSION_REQUEST: CreateSessionRequest = {
  mode: "libre",
  tools: [AGENT_DRAFT_TOOL],
};

/** One access of a draft. */
export interface ProposedAccess {
  id: string;
  /** Whether the form opens with it granted. A sensitive access never is. */
  checked: boolean;
  sensitive: boolean;
  /** Added because an agent created without code is granted it by default. */
  by_default: boolean;
}

/** The draft of an agent, as `propose_agent` answers it. */
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

// ── Reading the draft in the conversation ────────────────────────────────

function strings(value: unknown): string[] {
  return Array.isArray(value) ? value.filter((v): v is string => typeof v === "string") : [];
}

function accessOf(value: unknown): ProposedAccess[] {
  if (!Array.isArray(value)) return [];
  const out: ProposedAccess[] = [];
  for (const entry of value) {
    if (typeof entry !== "object" || entry === null) continue;
    const raw = entry as Record<string, unknown>;
    if (typeof raw.id !== "string" || raw.id.length === 0) continue;
    const sensitive = raw.sensitive === true;
    out.push({
      id: raw.id,
      // A sensitive access is never read as checked, whatever the answer says.
      checked: raw.checked === true && !sensitive,
      sensitive,
      by_default: raw.by_default === true,
    });
  }
  return out;
}

function draftOf(output: string | null): ProposedAgent | null {
  if (!output) return null;
  let parsed: unknown;
  try {
    parsed = JSON.parse(output);
  } catch {
    return null;
  }
  if (typeof parsed !== "object" || parsed === null) return null;
  const raw = (parsed as { draft?: unknown }).draft;
  if (typeof raw !== "object" || raw === null || Array.isArray(raw)) return null;
  const draft = raw as Record<string, unknown>;
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

/**
 * The latest draft of a conversation, or `null` when Apollia has not
 * proposed yet. Only a call that ran counts: a failed call proposes nothing.
 */
export function extractDraft(messages: ChatMessageView[]): ProposedAgent | null {
  for (let m = messages.length - 1; m >= 0; m -= 1) {
    const calls = messages[m].tool_calls ?? [];
    for (let c = calls.length - 1; c >= 0; c -= 1) {
      const call = calls[c];
      if (call.tool_name !== AGENT_DRAFT_TOOL || call.status !== "executed") continue;
      const draft = draftOf(call.output);
      if (draft) return draft;
    }
  }
  return null;
}

// ── The draft beside the conversation ────────────────────────────────────

/** The fields of a sheet, in the order the draft shows them. */
export const DRAFT_FIELDS = ["name", "mission", "examples", "model", "access", "sources"] as const;
export type DraftField = (typeof DRAFT_FIELDS)[number];

/** Where a field of the draft stands while the conversation goes on. */
export type DraftFieldState = "filled" | "writing" | "upcoming";

/** One field of the draft, as shown beside the conversation. */
export interface DraftFieldView {
  field: DraftField;
  state: DraftFieldState;
  /** The lines the field holds; empty until it is filled. */
  lines: string[];
  /** The model field only: the draft follows the default model. */
  defaultModel: boolean;
}

function fieldLines(draft: ProposedAgent, field: DraftField): string[] {
  switch (field) {
    case "name":
      return draft.name.trim() ? [draft.name.trim()] : [];
    case "mission":
      return draft.mission.trim() ? [draft.mission.trim()] : [];
    case "examples":
      return cleanExamples(draft.examples);
    case "model":
      return draft.model ? [draft.model] : [];
    case "access":
      return draft.access.map((a) => a.id);
    case "sources":
      return draft.sources;
  }
}

/**
 * The fields of the draft, each filled, being written or still to come.
 * Before the first proposal every field is to come, the first one being
 * written while Apollia answers. Once a draft exists the model is always
 * filled: with no model named, it follows the default.
 */
export function draftFields(draft: ProposedAgent | null, answering: boolean): DraftFieldView[] {
  let writingGiven = false;
  return DRAFT_FIELDS.map((field) => {
    const lines = draft ? fieldLines(draft, field) : [];
    const defaultModel = field === "model" && draft !== null && draft.model === null;
    if (lines.length > 0 || defaultModel) return { field, state: "filled", lines, defaultModel };
    if (answering && !writingGiven) {
      writingGiven = true;
      return { field, state: "writing", lines, defaultModel };
    }
    return { field, state: "upcoming", lines, defaultModel };
  });
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
 * The form a draft opens: its accesses listed, the sensitive ones unchecked,
 * its model kept only when the machine offers it. `null` opens an empty
 * form, for a sheet written by hand.
 */
export function reviewOf(draft: ProposedAgent | null, options: ModelOption[]): DraftReview {
  const access = draft?.access ?? [];
  const checked = access.filter((a) => a.checked && !a.sensitive).map((a) => a.id);
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

/** Label of each field of the draft. */
export const DRAFT_FIELD_LABEL: Record<DraftField, string> = {
  name: "agent_create.name_label",
  mission: "agent_adjust.mission_label",
  examples: "agent_create.field_examples",
  model: "agent_sheet.model_title",
  access: "agent_sheet.access_title",
  sources: "agent_sheet.sources_title",
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
