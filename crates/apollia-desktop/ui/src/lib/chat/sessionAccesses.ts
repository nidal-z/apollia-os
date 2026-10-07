/**
 * What the agent of a conversation reaches in it, as the Integrations tab of
 * the chat lists it.
 *
 * The list starts from the accesses of the agent in force. The conversation
 * switches some of them off for itself, and may switch on one the agent does
 * not hold: that one is proposed, and each of its calls asks for consent.
 * Nothing here writes to the agent or to the permissions; the conversation
 * stores two lists in its own settings.
 */
import type { SessionSettings } from "$lib/chat/sessionSettings";

/** A group of the tab, in the order the tab shows them. */
export const SESSION_ACCESS_KINDS = ["tools", "connections", "workflows", "agents"] as const;
export type SessionAccessKind = (typeof SESSION_ACCESS_KINDS)[number];

/**
 * Where an access stands in the conversation: held by the agent and on, held
 * and switched off here, not held, or not held and proposed here.
 */
export type SessionAccessState = "on" | "off" | "not_granted" | "proposed";

/** A tool as the tool list returns it. */
export interface ToolOffer {
  name: string;
  description: string;
  /** `native`, `mcp`, `python`. */
  kind: string;
}

/** An agent or a workflow the agent of the conversation may call. */
export interface TargetOffer {
  kind: "agent" | "workflow";
  /** The agent id, or `<package>/<workflow>`. */
  item_id: string;
  name: string;
  description: string;
}

/** The plain words of a native tool: the keys of its name and of what it does. */
export interface NativeWords {
  nameKey: string;
  detailKey: string;
}

/** One line of the tab. */
export interface SessionAccessRow {
  /** Raw identifier: shown in developer mode only. */
  id: string;
  kind: SessionAccessKind;
  /** The name read on the line, when it is not a catalogue key. */
  name: string;
  /** The catalogue key of the name of a native tool, `null` otherwise. */
  nameKey: string | null;
  /** What the access lets the agent do; empty when nothing describes it. */
  detail: string;
  /** The catalogue key of what a native tool does, `null` otherwise. */
  detailKey: string | null;
  /** Whether the agent holds the access. */
  held: boolean;
  state: SessionAccessState;
}

/** One group of the tab. */
export interface SessionAccessGroup {
  kind: SessionAccessKind;
  rows: SessionAccessRow[];
}

/** What the list is drawn from. */
export interface SessionAccessInput {
  /** The accesses of the agent in force. */
  held: readonly string[];
  /** Every tool of the machine. */
  tools: readonly ToolOffer[];
  /** Every agent and workflow that can be called. */
  targets: readonly TargetOffer[];
  /** The agent of the conversation: it is not offered to itself. */
  self: string;
  /** The plain words of the native tools, by identifier. */
  native?: ReadonlyMap<string, NativeWords>;
}

/** `file_read` reads `File read`; `agent:veille` reads `Veille`. */
function readable(raw: string): string {
  const words = raw.replace(/[._-]+/g, " ").trim();
  if (words.length === 0) return raw;
  return words.charAt(0).toUpperCase() + words.slice(1);
}

/** The name of a tool of a connection: `mcp:notion/search_pages` reads `Notion · Search pages`. */
function connectionName(id: string): string {
  const bare = id.slice(id.indexOf(":") + 1);
  const slash = bare.lastIndexOf("/");
  if (slash < 0) return readable(bare);
  return `${readable(bare.slice(0, slash))} · ${readable(bare.slice(slash + 1))}`;
}

function isConnection(id: string, tool: ToolOffer | undefined): boolean {
  if (id.startsWith("mcp:") || id.startsWith("conn:")) return true;
  return tool !== undefined && (tool.kind === "mcp" || tool.kind === "mcp_server");
}

/** The group an access is listed in. */
export function accessKind(id: string, tool?: ToolOffer): SessionAccessKind {
  if (id.startsWith("agent:")) return "agents";
  if (id.startsWith("workflow:")) return "workflows";
  return isConnection(id, tool) ? "connections" : "tools";
}

function stateOf(id: string, held: boolean, settings: SessionSettings): SessionAccessState {
  const off = settings.disabled_accesses?.includes(id) ?? false;
  if (held) return off ? "off" : "on";
  const proposed = settings.proposed_accesses?.includes(id) ?? false;
  return proposed && !off ? "proposed" : "not_granted";
}

/**
 * Every line of the tab: the accesses of the agent first, in its order, then
 * what it does not hold, by name. The agent is never offered to itself.
 */
export function sessionAccessRows(
  input: SessionAccessInput,
  settings: SessionSettings,
): SessionAccessRow[] {
  const tools = new Map(input.tools.map((tool) => [tool.name, tool]));
  const targets = new Map(input.targets.map((target) => [`${target.kind}:${target.item_id}`, target]));
  const held = [...new Set(input.held)];
  const heldSet = new Set(held);
  const selfId = `agent:${input.self}`;

  const row = (id: string, isHeld: boolean): SessionAccessRow => {
    const tool = tools.get(id);
    const target = targets.get(id);
    const kind = accessKind(id, tool);
    const words = kind === "tools" ? input.native?.get(id) : undefined;
    const bare = id.includes(":") ? id.slice(id.indexOf(":") + 1) : id;
    const name = target?.name ?? (kind === "connections" ? connectionName(id) : readable(bare));
    return {
      id,
      kind,
      name,
      nameKey: words?.nameKey ?? null,
      detail: (target?.description ?? tool?.description ?? "").trim(),
      detailKey: words?.detailKey ?? null,
      held: isHeld,
      state: stateOf(id, isHeld, settings),
    };
  };

  const offered = [
    ...input.tools.map((tool) => tool.name),
    ...input.targets.map((target) => `${target.kind}:${target.item_id}`),
  ].filter((id) => !heldSet.has(id) && id !== selfId);
  const others = [...new Set(offered)]
    .map((id) => row(id, false))
    .sort((a, b) => a.name.localeCompare(b.name));
  return [...held.filter((id) => id !== selfId).map((id) => row(id, true)), ...others];
}

/**
 * The lines whose name holds `query`, group by group; a group left without a
 * line is not shown. `name` gives the name read on a line, so a native tool
 * is found by its translated name; the raw identifier is searched only when
 * `withIds` says it is shown.
 */
export function groupSessionAccesses(
  rows: readonly SessionAccessRow[],
  query: string,
  name: (row: SessionAccessRow) => string = (row) => row.name,
  withIds = false,
): SessionAccessGroup[] {
  const q = query.trim().toLowerCase();
  const kept = rows.filter(
    (row) =>
      q.length === 0 ||
      name(row).toLowerCase().includes(q) ||
      (withIds && row.id.toLowerCase().includes(q)),
  );
  return SESSION_ACCESS_KINDS.map((kind) => ({
    kind,
    rows: kept.filter((row) => row.kind === kind),
  })).filter((group) => group.rows.length > 0);
}

/** Whether the switch of a line is on. */
export function isSwitchedOn(row: SessionAccessRow): boolean {
  return row.state === "on" || row.state === "proposed";
}

function without(list: readonly string[] | undefined, id: string): string[] {
  return (list ?? []).filter((entry) => entry !== id);
}

function withEntry(list: readonly string[] | undefined, id: string): string[] {
  return list?.includes(id) ? [...list] : [...(list ?? []), id];
}

function withList(
  settings: SessionSettings,
  key: "disabled_accesses" | "proposed_accesses",
  list: string[],
): SessionSettings {
  const next: SessionSettings = { ...settings };
  if (list.length === 0) delete next[key];
  else next[key] = list;
  return next;
}

/**
 * The settings of the conversation once the switch of `row` is moved. An
 * access of the agent is switched off, or back on, for the conversation; an
 * access the agent does not hold is proposed, or no longer. Every other
 * setting is carried over untouched.
 */
export function switchSessionAccess(
  settings: SessionSettings,
  row: Pick<SessionAccessRow, "id" | "held">,
  on: boolean,
): SessionSettings {
  if (row.held) {
    const disabled = on
      ? without(settings.disabled_accesses, row.id)
      : withEntry(settings.disabled_accesses, row.id);
    return withList(settings, "disabled_accesses", disabled);
  }
  const cleared = withList(settings, "disabled_accesses", without(settings.disabled_accesses, row.id));
  const proposed = on
    ? withEntry(cleared.proposed_accesses, row.id)
    : without(cleared.proposed_accesses, row.id);
  return withList(cleared, "proposed_accesses", proposed);
}

/** The badge a line carries, `null` for an access that is simply on. */
export const SESSION_ACCESS_BADGE: Record<SessionAccessState, string | null> = {
  on: null,
  off: "chat.integrations.badge_off",
  not_granted: "chat.integrations.badge_not_granted",
  proposed: "chat.integrations.badge_proposed",
};

/** The sentence under a line the agent does not hold, `null` otherwise. */
export const SESSION_ACCESS_NOTE: Record<SessionAccessState, string | null> = {
  on: null,
  off: null,
  not_granted: "chat.integrations.note_not_granted",
  proposed: "chat.integrations.note_proposed",
};

/** Title of each group. */
export const SESSION_ACCESS_KIND_LABEL: Record<SessionAccessKind, string> = {
  tools: "chat.integrations.group_tools",
  connections: "chat.integrations.group_connections",
  workflows: "chat.integrations.group_workflows",
  agents: "chat.integrations.group_agents",
};

// ── Profile ─────────────────────────────────────────────────────────────

/**
 * Which profile switch a conversation shows. With the built-in assistant the
 * profile is added to the conversation; an installed agent may read it, at
 * its own initiative, and it is never added to what the agent is sent.
 */
export type ProfileSwitchCase = "assistant" | "agent";

export function profileSwitchCase(
  session: { mode: "libre" | "agent"; agent_name: string | null } | null,
): ProfileSwitchCase {
  return session?.mode === "agent" && session.agent_name ? "agent" : "assistant";
}

/**
 * Whether an installed agent may read the profile in this conversation: the
 * choice of the conversation when it made one, the profile source of the
 * agent otherwise.
 */
export function agentProfileSwitch(settings: SessionSettings, sources: readonly string[]): boolean {
  return settings.profile_access ?? sources.includes("profile");
}

/** The label of the switch, and the sentence under it, for each case. */
export const PROFILE_SWITCH_KEYS: Record<ProfileSwitchCase, { label: string; help: string }> = {
  assistant: {
    label: "chat.integrations.profile_switch_assistant",
    help: "chat.integrations.profile_switch_assistant_help",
  },
  agent: {
    label: "chat.integrations.profile_switch_agent",
    help: "chat.integrations.profile_switch_agent_help",
  },
};
