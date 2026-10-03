/**
 * The decisions behind the adjustment of an agent on its sheet, kept apart
 * from the markup so they are tested on their own (Vitest runs in `node`).
 *
 * The sheet is edited as one draft: mission and examples, model and preset,
 * accesses, sources. Saving turns the difference between the agent in force
 * and the draft into writes, one per changed field: an override set, an
 * override removed when the draft is back on the package's value, the model
 * assigned or cleared. Nothing here calls the runtime.
 */
import type {
  AgentModelAssignment,
  ModelSource,
  ResolvedAgent,
} from "$lib/ipc/agentSheet";
import type { LlmBackendConfig } from "$lib/types";

/** The fields of an agent the sheet writes as overrides. */
export type AdjustableField = "mission" | "examples" | "access" | "sources";

/** Every field the sheet marks as the user's: the overrides, and the model. */
export type MarkedField = AdjustableField | "model";

/** The model chosen on the sheet; `null` follows the default model. */
export interface ModelDraft {
  model_source: ModelSource;
  /** The backend that serves it, as the router names it. */
  model_id: string;
  /** Preset of the model; `null` keeps the model's default. */
  preset_id: number | null;
}

/** What the user is changing on the sheet. */
export interface AdjustDraft {
  mission: string;
  examples: string[];
  access: string[];
  sources: string[];
  model: ModelDraft | null;
}

/** One write a save sends to the runtime. */
export type AdjustWrite =
  | { kind: "set_override"; field: AdjustableField; value: string | string[] }
  | { kind: "remove_override"; field: AdjustableField }
  | { kind: "set_model"; model: ModelDraft }
  | { kind: "clear_model" };

/** The sources the sheet always offers to check, whatever the agent holds. */
export const OFFERED_SOURCES: readonly string[] = ["profile", "memory"];

// ── Draft ───────────────────────────────────────────────────────────────

function modelDraft(model: AgentModelAssignment | null): ModelDraft | null {
  if (!model) return null;
  return { model_source: model.model_source, model_id: model.model_id, preset_id: model.preset_id };
}

/** The draft an adjustment starts from: the agent in force. */
export function draftFrom(agent: ResolvedAgent): AdjustDraft {
  return {
    mission: agent.mission,
    examples: [...agent.examples],
    access: [...agent.access],
    sources: [...agent.sources],
    model: modelDraft(agent.model),
  };
}

/** The values the package declares, before any override. */
export function packageDefaults(agent: ResolvedAgent): Record<AdjustableField, string | string[]> {
  const definition = (agent.definition ?? {}) as Record<string, unknown>;
  const defaults = (definition.defaults ?? {}) as Record<string, unknown>;
  const list = (value: unknown): string[] =>
    Array.isArray(value) ? value.filter((v): v is string => typeof v === "string") : [];
  return {
    mission: typeof definition.mission === "string" ? definition.mission : "",
    examples: list(definition.examples),
    access: list(defaults.access),
    sources: list(defaults.sources),
  };
}

// ── Comparison ──────────────────────────────────────────────────────────

/** Examples as they are saved: trimmed, the empty ones dropped, in order. */
export function cleanExamples(examples: string[]): string[] {
  return examples.map((e) => e.trim()).filter((e) => e.length > 0);
}

function sameSet(a: string[], b: string[]): boolean {
  const left = new Set(a);
  const right = new Set(b);
  return left.size === right.size && [...left].every((v) => right.has(v));
}

function sameList(a: string[], b: string[]): boolean {
  return a.length === b.length && a.every((v, i) => v === b[i]);
}

function normalized(field: AdjustableField, value: string | string[]): string | string[] {
  if (field === "mission") return (value as string).trim();
  if (field === "examples") return cleanExamples(value as string[]);
  return [...new Set(value as string[])];
}

function sameValue(field: AdjustableField, a: string | string[], b: string | string[]): boolean {
  const left = normalized(field, a);
  const right = normalized(field, b);
  if (field === "mission") return left === right;
  if (field === "examples") return sameList(left as string[], right as string[]);
  return sameSet(left as string[], right as string[]);
}

function sameModel(a: ModelDraft | null, b: ModelDraft | null): boolean {
  if (a === null || b === null) return a === b;
  return a.model_source === b.model_source && a.model_id === b.model_id && a.preset_id === b.preset_id;
}

const FIELDS: readonly AdjustableField[] = ["mission", "examples", "access", "sources"];

/**
 * The writes that turn the agent in force into the draft, one per changed
 * field, in the order of the sheet. A field brought back to the package's
 * value removes its override rather than writing the same value as the user's.
 */
export function adjustmentWrites(agent: ResolvedAgent, draft: AdjustDraft): AdjustWrite[] {
  const current = draftFrom(agent);
  const defaults = packageDefaults(agent);
  const writes: AdjustWrite[] = [];
  for (const field of FIELDS) {
    if (sameValue(field, current[field], draft[field])) continue;
    if (agent.overridden.includes(field) && sameValue(field, defaults[field], draft[field])) {
      writes.push({ kind: "remove_override", field });
    } else {
      writes.push({ kind: "set_override", field, value: normalized(field, draft[field]) });
    }
  }
  if (!sameModel(current.model, draft.model)) {
    writes.push(draft.model ? { kind: "set_model", model: draft.model } : { kind: "clear_model" });
  }
  return writes;
}

/** The fields the draft changes, for the "Modified" marks of the session. */
export function changedFields(agent: ResolvedAgent, draft: AdjustDraft): Set<MarkedField> {
  return new Set(
    adjustmentWrites(agent, draft).map((w) =>
      w.kind === "set_model" || w.kind === "clear_model" ? "model" : w.field,
    ),
  );
}

/** Whether the value of `field` in force is the user's rather than the package's. */
export function isOverridden(agent: ResolvedAgent | null, field: MarkedField): boolean {
  if (!agent) return false;
  if (field === "model") return agent.model !== null;
  return agent.overridden.includes(field);
}

/** Whether the agent holds anything "Back to defaults" would remove. */
export function hasOverrides(agent: ResolvedAgent | null): boolean {
  return agent !== null && (agent.overridden.length > 0 || agent.model !== null);
}

// ── Accesses ────────────────────────────────────────────────────────────

/** Grants `id` in the draft; granting twice changes nothing. */
export function grant(access: string[], id: string): string[] {
  return access.includes(id) ? access : [...access, id];
}

/** Revokes `id` in the draft. */
export function revoke(access: string[], id: string): string[] {
  return access.filter((a) => a !== id);
}

/**
 * The accesses the adjustment lists: those in force, then those granted
 * during the session. One revoked during the session stays listed, its
 * switch off, so it can be granted back before saving.
 */
export function listedAccesses(inForce: string[], draft: string[]): string[] {
  return [...new Set([...inForce, ...draft])];
}

/** What can be granted from the sheet's dialog. */
export type GrantKind = "tool" | "agent" | "workflow";

/** One thing the dialog offers to grant. */
export interface GrantCandidate {
  /** The access identifier the agent is granted. */
  id: string;
  kind: GrantKind;
  name: string;
  description: string;
  /** Already granted in the draft. */
  granted: boolean;
}

/** A tool as the tool list returns it. */
export interface ToolOffer {
  name: string;
  description: string;
}

/** An agent or a workflow another agent may call. */
export interface TargetOffer {
  kind: "agent" | "workflow";
  /** The agent id, or `<package>/<workflow>`. */
  item_id: string;
  name: string;
  description: string;
}

/**
 * Everything the dialog offers: the tools (native, of a connection or of a
 * package), the other agents as `agent:<id>`, the workflows as
 * `workflow:<package>/<name>`. The agent being adjusted is not offered to
 * itself.
 */
export function grantCandidates(
  tools: ToolOffer[],
  targets: TargetOffer[],
  granted: string[],
  self: string,
): GrantCandidate[] {
  const has = new Set(granted);
  const toolRows: GrantCandidate[] = [...tools]
    .sort((a, b) => a.name.localeCompare(b.name))
    .map((t) => ({ id: t.name, kind: "tool", name: t.name, description: t.description, granted: has.has(t.name) }));
  const targetRows: GrantCandidate[] = targets
    .filter((t) => !(t.kind === "agent" && t.item_id === self))
    .map((t) => {
      const id = `${t.kind}:${t.item_id}`;
      return { id, kind: t.kind, name: t.name, description: t.description, granted: has.has(id) };
    })
    .sort((a, b) => a.name.localeCompare(b.name));
  return [...toolRows, ...targetRows];
}

/** The candidates whose name, description or identifier holds `query`. */
export function filterCandidates(candidates: GrantCandidate[], query: string): GrantCandidate[] {
  const q = query.trim().toLowerCase();
  if (q.length === 0) return candidates;
  return candidates.filter(
    (c) =>
      c.name.toLowerCase().includes(q) ||
      c.description.toLowerCase().includes(q) ||
      c.id.toLowerCase().includes(q),
  );
}

// ── Sources ─────────────────────────────────────────────────────────────

/** The sources the adjustment lists, each checked or not. */
export function sourceChoices(inForce: string[], draft: string[]): { id: string; checked: boolean }[] {
  const ids = [...new Set([...OFFERED_SOURCES, ...inForce, ...draft])];
  return ids.map((id) => ({ id, checked: draft.includes(id) }));
}

/** Checks or unchecks `id` in the draft. */
export function toggleSource(sources: string[], id: string, checked: boolean): string[] {
  if (checked) return sources.includes(id) ? sources : [...sources, id];
  return sources.filter((s) => s !== id);
}

// ── Model ───────────────────────────────────────────────────────────────

/** One model the sheet offers. */
export interface ModelOption {
  /** The backend name, written as the model of the agent. */
  id: string;
  source: ModelSource;
  /** The model the backend serves. */
  model: string;
}

/** The enabled backends, by name, each with where its model runs. */
export function modelOptions(backends: LlmBackendConfig[]): ModelOption[] {
  return backends
    .filter((b) => b.enabled)
    .sort((a, b) => a.name.localeCompare(b.name))
    .map((b) => ({ id: b.name, source: b.provider === "llama-cpp" ? "local" : "server", model: b.model }));
}

/** A preset as `list_model_presets` returns it. */
export interface PresetOffer {
  id: number;
  name: string;
  model_source: ModelSource;
  model_id: string;
  is_model_default: boolean;
}

/** The presets of the chosen model. */
export function presetsOf(presets: PresetOffer[], model: ModelDraft | null): PresetOffer[] {
  if (!model) return [];
  return presets.filter((p) => p.model_source === model.model_source && p.model_id === model.model_id);
}

/**
 * The model draft once a backend is chosen in the select: `""` follows the
 * default model; a new model drops a preset that belonged to the previous one.
 */
export function chooseModel(options: ModelOption[], current: ModelDraft | null, id: string): ModelDraft | null {
  if (id === "") return null;
  const option = options.find((o) => o.id === id);
  const source: ModelSource = option?.source ?? current?.model_source ?? "local";
  const preset = current && current.model_id === id ? current.preset_id : null;
  return { model_source: source, model_id: id, preset_id: preset };
}

// ── Catalogue keys ──────────────────────────────────────────────────────

/** Title of each group of the grant dialog. */
export const GRANT_GROUP_LABEL: Record<GrantKind, string> = {
  tool: "agent_adjust.grant_group_tools",
  agent: "agent_adjust.grant_group_agents",
  workflow: "agent_adjust.grant_group_workflows",
};
