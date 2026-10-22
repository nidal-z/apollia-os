/**
 * What the Chat offers when Apollia looked for a connection it lacks: add the
 * service that is not connected, or grant Apollia the one that is, then play
 * the request again.
 *
 * Pure and DOM-free, except for `proposeConnection`, whose commands are
 * injectable. The offer component renders these decisions.
 */
import type { SessionSettings } from "$lib/chat/sessionSettings";
import { connectionTools } from "$lib/connections/addFlow";
import { parseJsonObject } from "$lib/chat/toolBodies";
import { getChatSessionSettings, setChatSessionSettings } from "$lib/ipc/chatSettings";
import { listConnections } from "$lib/ipc/connections";
import type { Connection } from "$lib/types";

/** The name the tool that looks for a missing connection is called by. */
export const CONNECTION_LOOKUP_TOOL = "connection_lookup";

/**
 * One offer: an entry of the selection that is not connected, to be added,
 * or a connection made from it that Apollia is not granted, to be granted.
 */
export type ConnectionOffer =
  | { kind: "add"; entry: string; label: string }
  | { kind: "grant"; entry: string; label: string; connection: string };

/**
 * The offers a `connection_lookup` result makes, in the order of its
 * matches. An entry already granted offers nothing; an output that is not
 * the tool's result offers nothing either.
 */
export function connectionOffers(output: string | null): ConnectionOffer[] {
  const obj = parseJsonObject(output);
  if (!obj || !Array.isArray(obj.matches)) return [];
  const offers: ConnectionOffer[] = [];
  const seen = new Set<string>();
  for (const raw of obj.matches) {
    if (typeof raw !== "object" || raw === null) continue;
    const m = raw as Record<string, unknown>;
    if (typeof m.entry !== "string" || m.entry.length === 0 || seen.has(m.entry)) continue;
    const label = typeof m.label === "string" && m.label.length > 0 ? m.label : m.entry;
    if (m.status === "not_connected") {
      offers.push({ kind: "add", entry: m.entry, label });
    } else if (m.status === "connected_not_granted" && typeof m.connection === "string") {
      offers.push({ kind: "grant", entry: m.entry, label, connection: m.connection });
    } else {
      continue;
    }
    seen.add(m.entry);
  }
  return offers;
}

/**
 * The settings of the conversation with `tools` proposed in it: the model may
 * call them in this conversation, and each call asks for consent when the
 * agent does not hold them. Every other setting is carried over.
 */
export function withProposedTools(settings: SessionSettings, tools: readonly string[]): SessionSettings {
  const proposed = [...(settings.proposed_accesses ?? [])];
  for (const tool of tools) if (!proposed.includes(tool)) proposed.push(tool);
  const disabled = (settings.disabled_accesses ?? []).filter((id) => !tools.includes(id));
  const next: SessionSettings = { ...settings, proposed_accesses: proposed };
  if (disabled.length > 0) next.disabled_accesses = disabled;
  else delete next.disabled_accesses;
  if (proposed.length === 0) delete next.proposed_accesses;
  return next;
}

export interface ProposeDeps {
  list?: () => Promise<Connection[]>;
  read?: (sessionId: string) => Promise<SessionSettings>;
  write?: (sessionId: string, settings: SessionSettings) => Promise<SessionSettings>;
  /** How many times the connection is read while it offers no tool yet. */
  attempts?: number;
  waitMs?: number;
}

/**
 * Proposes every tool of the connection `connectionId` to the conversation
 * `sessionId`, through its own settings: nothing is written to the agent.
 * A server just started may not have listed its tools yet, so the list is
 * read again a few times while it offers none. Returns how many tools were
 * proposed; the settings are left alone when there is none.
 */
export async function proposeConnection(
  sessionId: string,
  connectionId: string,
  deps: ProposeDeps = {},
): Promise<number> {
  const list = deps.list ?? listConnections;
  const read = deps.read ?? getChatSessionSettings;
  const write = deps.write ?? setChatSessionSettings;
  const attempts = Math.max(1, deps.attempts ?? 6);
  const waitMs = deps.waitMs ?? 500;
  let tools: string[] = [];
  for (let i = 0; i < attempts; i += 1) {
    tools = connectionTools(await list(), connectionId);
    if (tools.length > 0) break;
    if (i + 1 < attempts) await new Promise((resolve) => setTimeout(resolve, waitMs));
  }
  if (tools.length === 0) return 0;
  await write(sessionId, withProposedTools(await read(sessionId), tools));
  return tools.length;
}
