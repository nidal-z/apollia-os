/**
 * The decisions behind the sheet of an agent, kept apart from the markup so
 * each section is tested on its own (Vitest runs in `node` here).
 *
 * Nothing in this module translates: it returns the i18n keys and the values
 * the sections render, and the sections call `$t`.
 */
import type { ActionFamily, FamilySource } from "$lib/tools/action-family";
import { isSensitiveFamily } from "$lib/tools/action-family";
import type {
  AgentChangeEntry,
  AgentModelAssignment,
  AgentModelFallback,
} from "$lib/ipc/agentSheet";
import type { MemoryEntry, MemorySearchResult } from "$lib/types";

/** The sections of the sheet, in the order they are read. */
export const SHEET_SECTIONS = [
  "mission",
  "model",
  "access",
  "sources",
  "activity",
  "memory",
] as const;
export type SheetSection = (typeof SHEET_SECTIONS)[number];

/** Identifier of the built-in Apollia agent. */
export const APOLLIA_AGENT_ID = "apollia:chat";

// ── Mission ─────────────────────────────────────────────────────────────

/** What the mission section shows. */
export interface MissionView {
  /** The mission, `null` when neither the package nor the user wrote one. */
  mission: string | null;
  examples: string[];
}

/**
 * The mission and the examples, from the resolved agent first, then from what
 * the agent's own code declares when the runtime holds no definition of it.
 */
export function missionView(
  resolved: { mission: string; examples: string[] } | null,
  declared: { description: string | null; examples: string[] } | null,
): MissionView {
  const mission = (resolved?.mission ?? "").trim() || (declared?.description ?? "").trim();
  const examples = resolved && resolved.examples.length > 0 ? resolved.examples : (declared?.examples ?? []);
  return { mission: mission.length > 0 ? mission : null, examples };
}

// ── Model ───────────────────────────────────────────────────────────────

/** Where the model of an agent runs. */
export type ModelPlace = "local" | "server" | "default";

/** What the model section shows. */
export interface ModelView {
  /** Name of the model, `null` when the default is not known. */
  name: string | null;
  place: ModelPlace;
  /** The server a remote model runs on. */
  server: string | null;
  /** Whether a preset other than the model's default is chosen. */
  preset: number | null;
}

/**
 * The model of the agent: its assignment in `agent_model` when there is one,
 * otherwise the default model of the machine, which it then follows.
 */
export function modelView(
  assignment: AgentModelAssignment | null,
  defaultModel: string | null,
): ModelView {
  if (!assignment) {
    return { name: defaultModel, place: "default", server: null, preset: null };
  }
  if (assignment.model_source === "server") {
    const slash = assignment.model_id.indexOf("/");
    const server = slash > 0 ? assignment.model_id.slice(0, slash) : null;
    const name = slash > 0 ? assignment.model_id.slice(slash + 1) : assignment.model_id;
    return { name, place: "server", server, preset: assignment.preset_id };
  }
  return { name: assignment.model_id, place: "local", server: null, preset: assignment.preset_id };
}

/** The fallback notice, `null` when the model never fell back. */
export function fallbackNotice(
  fallback: AgentModelFallback | null,
): { requested: string; used: string; at: string } | null {
  if (!fallback) return null;
  return { requested: fallback.requested, used: fallback.used, at: fallback.at };
}

// ── Access ──────────────────────────────────────────────────────────────

/** A group of accesses on the sheet: an action family, or the agents and workflows. */
export type AccessGroupKey =
  | "read"
  | "read_personal"
  | "write_or_delete"
  | "send_external"
  | "execute_code"
  | "agents"
  | "unclassified";

/** The groups always shown, in order; `unclassified` only when it holds something. */
export const ACCESS_GROUP_ORDER: readonly AccessGroupKey[] = [
  "read",
  "read_personal",
  "write_or_delete",
  "send_external",
  "execute_code",
  "agents",
  "unclassified",
];

/** Where an access comes from, shown in developer mode only. */
export type AccessProvenance = "builtin" | "connection" | "package" | "agent" | "workflow" | "unknown";

/** The part of a tool descriptor the sheet reads. */
export interface AccessDescriptor {
  description: string;
  family: ActionFamily;
  family_source: FamilySource;
  /** Discriminant of the `ToolKind`: `native`, `mcp_server`, `python`, ... */
  kind: string | null;
}

/** One access granted to the agent, as the sheet shows it. */
export interface AccessView {
  /** Raw identifier: shown in developer mode only. */
  id: string;
  /** What the access lets the agent do, in plain words. */
  permits: string | null;
  /** A readable name derived from the identifier, for an access with no description. */
  label: string;
  group: AccessGroupKey;
  provenance: AccessProvenance;
  sensitive: boolean;
}

/** `file_read` reads `File read`; `mcp:outlook` reads `outlook`. */
export function readableName(id: string): string {
  const bare = id.includes(":") ? id.slice(id.lastIndexOf(":") + 1) : id;
  const words = bare.replace(/[._-]+/g, " ").trim();
  if (words.length === 0) return id;
  return words.charAt(0).toUpperCase() + words.slice(1);
}

function provenanceOf(id: string, descriptor: AccessDescriptor | null): AccessProvenance {
  if (id.startsWith("agent:")) return "agent";
  if (id.startsWith("workflow:")) return "workflow";
  if (id.startsWith("mcp:")) return "connection";
  if (!descriptor) return "unknown";
  if (descriptor.family_source === "package" || descriptor.kind === "python") return "package";
  if (descriptor.kind === "mcp_server" || descriptor.kind === "mcp") return "connection";
  if (descriptor.family_source === "server" || descriptor.family_source === "selection") {
    return "connection";
  }
  return "builtin";
}

function groupOf(id: string, descriptor: AccessDescriptor | null): AccessGroupKey {
  if (id.startsWith("agent:") || id.startsWith("workflow:")) return "agents";
  const family = descriptor?.family ?? "unclassified";
  switch (family) {
    case "read":
    case "exempt":
      return "read";
    case "read_personal":
    case "write_or_delete":
    case "send_external":
    case "execute_code":
      return family;
    default:
      return "unclassified";
  }
}

/** The sheet's view of one access, from its identifier and its descriptor if any. */
export function accessView(id: string, descriptor: AccessDescriptor | null): AccessView {
  const group = groupOf(id, descriptor);
  const permits = descriptor && descriptor.description.trim().length > 0 ? descriptor.description.trim() : null;
  return {
    id,
    permits,
    label: readableName(id),
    group,
    provenance: provenanceOf(id, descriptor),
    sensitive: group === "agents" ? false : isSensitiveFamily(descriptor?.family ?? "unclassified"),
  };
}

/** One group of the access section. */
/** Where the access of an agent comes from, as its sheet says it. */
export interface AccessOrigin {
  /** The agent holds the default access, read again at every run. */
  byDefault: boolean;
  /** The accesses given on top of the default access. */
  added: Set<string>;
}

/** Whether `resolved` holds the default access, and what it was given on top of it. */
export function accessOrigin(
  resolved: { access_by_default?: boolean; access_added?: string[] } | null,
): AccessOrigin {
  const byDefault = resolved?.access_by_default === true;
  return { byDefault, added: new Set(byDefault ? (resolved?.access_added ?? []) : []) };
}

export interface AccessGroup {
  key: AccessGroupKey;
  accesses: AccessView[];
}

/**
 * The accesses grouped by action family, whatever their provenance. Every
 * family is listed, empty or not, so the reader sees what the agent cannot
 * do as plainly as what it can; the unclassified group only when it holds one.
 */
export function groupAccesses(accesses: AccessView[]): AccessGroup[] {
  return ACCESS_GROUP_ORDER.map((key) => ({
    key,
    accesses: accesses.filter((a) => a.group === key),
  })).filter((group) => group.key !== "unclassified" || group.accesses.length > 0);
}

// ── Sources ─────────────────────────────────────────────────────────────

/** Whether the agent may read the user's profile. */
export function readsProfile(sources: string[]): boolean {
  return sources.includes("profile");
}

// ── Activity: changes ───────────────────────────────────────────────────

/** What one change of the governance history says. */
export type ChangeKind =
  | "access_granted"
  | "access_revoked"
  | "access_rescoped"
  | "override_set"
  | "override_removed"
  | "installed"
  | "updated"
  | "uninstalled"
  | "other";

/** One line of the changes of an agent. */
export interface ChangeView {
  key: string;
  kind: ChangeKind;
  /** The access or the field the change is about. */
  subject: string | null;
  at: string;
  actor: string | null;
  /** Versions of an update, `from` then `to`. */
  versions: [string, string] | null;
}

function text(value: unknown): string | null {
  return typeof value === "string" && value.length > 0 ? value : null;
}

/** The sheet's view of one journal entry of the governance history. */
export function changeView(entry: AgentChangeEntry): ChangeView {
  const p = entry.payload ?? {};
  let kind: ChangeKind = "other";
  let subject: string | null = null;
  let versions: [string, string] | null = null;
  switch (entry.kind) {
    case "access_changed": {
      const change = text(p.change);
      kind = change === "revoked" ? "access_revoked" : change === "rescoped" ? "access_rescoped" : "access_granted";
      subject = text(p.access_id);
      break;
    }
    case "agent_override_changed":
      kind = text(p.change) === "removed" ? "override_removed" : "override_set";
      subject = text(p.field);
      break;
    case "agent_installed":
      kind = "installed";
      subject = text(p.version);
      break;
    case "agent_updated": {
      kind = "updated";
      const from = text(p.from_version);
      const to = text(p.to_version);
      versions = from && to ? [from, to] : null;
      break;
    }
    case "agent_uninstalled":
      kind = "uninstalled";
      subject = text(p.version);
      break;
  }
  return { key: `${entry.run_id}#${entry.seq}`, kind, subject, at: entry.ts, actor: text(p.actor), versions };
}

/** The changes of an agent, newest first. */
export function changeViews(entries: AgentChangeEntry[]): ChangeView[] {
  return [...entries].sort((a, b) => b.seq - a.seq).map(changeView);
}

// ── Memory ──────────────────────────────────────────────────────────────

/** One thing the agent retained. */
export interface MemoryRow {
  id: string;
  /** What was retained. */
  text: string;
  /** The key it was retained under, shown in developer mode only. */
  key: string | null;
  at: string;
}

/** The rows of a listing, newest first. */
export function memoryRowsFromEntries(entries: MemoryEntry[]): MemoryRow[] {
  return [...entries]
    .sort((a, b) => b.created_at.localeCompare(a.created_at))
    .map((e) => ({ id: e.id, text: e.value || e.key, key: e.key, at: e.created_at }));
}

/** The rows of a search, best match first as the search ranks them. */
export function memoryRowsFromSearch(results: MemorySearchResult[]): MemoryRow[] {
  return results.map((r) => ({ id: r.id, text: r.content, key: null, at: r.created_at }));
}

/** The listing once one entry is deleted. */
export function withoutRow(rows: MemoryRow[], id: string): MemoryRow[] {
  return rows.filter((r) => r.id !== id);
}

// ── Catalogue keys ──────────────────────────────────────────────────────
// Spelled out rather than interpolated, so the catalogue guards see every key
// the sheet reads.

/** Title of each access group. */
export const ACCESS_GROUP_LABEL: Record<AccessGroupKey, string> = {
  read: "agent_sheet.family_read",
  read_personal: "agent_sheet.family_read_personal",
  write_or_delete: "agent_sheet.family_write_or_delete",
  send_external: "agent_sheet.family_send_external",
  execute_code: "agent_sheet.family_execute_code",
  agents: "agent_sheet.family_agents",
  unclassified: "agent_sheet.family_unclassified",
};

/** The rule a group of accesses is used under. */
export const ACCESS_GROUP_RULE: Record<AccessGroupKey, string> = {
  read: "agent_sheet.rule_without_asking",
  read_personal: "agent_sheet.rule_asks_first",
  write_or_delete: "agent_sheet.rule_asks_first",
  send_external: "agent_sheet.rule_asks_first",
  execute_code: "agent_sheet.rule_asks_each_time",
  agents: "agent_sheet.rule_callee_accesses",
  unclassified: "agent_sheet.rule_unclassified",
};

/** Name of each provenance, for developer mode. */
export const PROVENANCE_LABEL: Record<AccessProvenance, string> = {
  builtin: "agent_sheet.provenance_builtin",
  connection: "agent_sheet.provenance_connection",
  package: "agent_sheet.provenance_package",
  agent: "agent_sheet.provenance_agent",
  workflow: "agent_sheet.provenance_workflow",
  unknown: "agent_sheet.provenance_unknown",
};

/** Sentence of each kind of change; `{subject}` is the access or the setting. */
export const CHANGE_LABEL: Record<ChangeKind, string> = {
  access_granted: "agent_sheet.change_access_granted",
  access_revoked: "agent_sheet.change_access_revoked",
  access_rescoped: "agent_sheet.change_access_rescoped",
  override_set: "agent_sheet.change_override_set",
  override_removed: "agent_sheet.change_override_removed",
  installed: "agent_sheet.change_installed",
  updated: "agent_sheet.change_updated",
  uninstalled: "agent_sheet.change_uninstalled",
  other: "agent_sheet.change_other",
};

/** Name of each setting a change can be about. */
export const FIELD_LABEL: Record<string, string> = {
  name: "agent_sheet.field_name",
  mission: "agent_sheet.field_mission",
  examples: "agent_sheet.field_examples",
  access: "agent_sheet.field_access",
  sources: "agent_sheet.field_sources",
  autonomy: "agent_sheet.field_autonomy",
  profile: "agent_sheet.field_profile",
  model: "agent_sheet.field_model",
};
