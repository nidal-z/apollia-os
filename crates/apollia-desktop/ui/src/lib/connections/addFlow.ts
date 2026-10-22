/**
 * The one flow that adds a connection, whatever its kind: connect it (sign in,
 * pick the folder, type the address, accept the warning), choose who may use
 * it, then try it.
 *
 * Pure and DOM-free, except for the grant, whose two commands are injectable.
 * The flow component renders these decisions; nothing here reads a store.
 */
import { APOLLIA_AGENT_ID } from "$lib/chat/chatTargets";
import { grantConnection, listConnections } from "$lib/ipc/connections";
import { sanitizeServerName } from "$lib/integrations/connectorConfig";
import type { AddEntry } from "$lib/connections/addWindow";
import type { AgentListItem, Connection, McpServerConfigInput } from "$lib/types";

/** The three steps every addition goes through, in order. */
export const FLOW_STEPS = ["connect", "who", "trial"] as const;
export type FlowStep = (typeof FLOW_STEPS)[number];

/**
 * Where an addition stands. `connect` waits for the gesture of its kind,
 * `working` runs on its own (the addition, a download, the trial), `who` waits
 * for the agents, `trial` shows how the connection answered. `refused` and
 * `failed` end the addition before the connection exists.
 */
export type FlowPhase = "connect" | "working" | "who" | "trial" | "refused" | "failed";

/** The step a phase belongs to, for the row of steps above the flow. */
export function stepOf(phase: FlowPhase): FlowStep {
  switch (phase) {
    case "who":
      return "who";
    case "trial":
      return "trial";
    default:
      return "connect";
  }
}

/** What the connect step of an entry asks for, before anything is written. */
export type ConnectGesture = "warning" | "address" | "mailbox" | "folder" | "sign_in";

export function connectGesture(entry: AddEntry): ConnectGesture {
  switch (entry.kind) {
    case "catalogue":
      return "warning";
    case "address":
      return "address";
    case "mailbox":
      return "mailbox";
    case "folder":
      return "folder";
    case "native":
      return "sign_in";
  }
}

/** The connection id of the MCP server `name`. */
export function mcpConnectionId(name: string): string {
  return `mcp:${name}`;
}

/** The connection id of the account `accountId` of a native provider. */
export function nativeConnectionId(provider: string, accountId: string): string {
  return `native:${provider}:${accountId}`;
}

/** One agent the "who may use it" step offers. */
export interface WhoChoice {
  id: string;
  name: string;
  /** Apollia, the agent of the Chat, comes first and checked. */
  apollia: boolean;
}

/**
 * The agents offered: Apollia first, then every other agent of this machine
 * by name, the system agents left out. Only Apollia starts checked.
 */
export function whoChoices(agents: readonly AgentListItem[], apolliaName: string): WhoChoice[] {
  const others = agents
    .filter((a) => a.agent_type !== "system" && a.name !== APOLLIA_AGENT_ID)
    .map((a) => ({ id: a.name, name: a.name, apollia: false }))
    .sort((a, b) => a.name.localeCompare(b.name));
  const seen = new Set<string>();
  const unique = others.filter((c) => (seen.has(c.id) ? false : (seen.add(c.id), true)));
  return [{ id: APOLLIA_AGENT_ID, name: apolliaName, apollia: true }, ...unique];
}

/** The agents checked when the step opens: Apollia alone. */
export function initiallyChecked(): string[] {
  return [APOLLIA_AGENT_ID];
}

/** Every tool of the connection `id`, none when it is not listed. */
export function connectionTools(connections: readonly Connection[], id: string): string[] {
  return connections.find((c) => c.id === id)?.tools.map((tool) => tool.id) ?? [];
}

export interface GrantDeps {
  list?: () => Promise<Connection[]>;
  grant?: (connectionId: string, agents: string[], tools: string[]) => Promise<unknown>;
  /** How many times the connection is read while it offers no tool yet. */
  attempts?: number;
  waitMs?: number;
}

/**
 * Gives `agents` every tool of the connection `connectionId`, through the
 * grant route of the runtime, which journals it and keeps an agent on the
 * default access on it. What a tool may do is still decided at each call: a
 * sensitive one, and one nobody classified, asks for approval first.
 *
 * A server just started may not have indexed its tools yet, so the list is
 * read again a few times while the connection offers none. Returns how many
 * tools were granted; nothing is written when no agent is checked.
 */
export async function grantToAgents(
  connectionId: string,
  agents: readonly string[],
  deps: GrantDeps = {},
): Promise<number> {
  if (agents.length === 0) return 0;
  const list = deps.list ?? listConnections;
  const grant = deps.grant ?? grantConnection;
  const attempts = Math.max(1, deps.attempts ?? 6);
  const waitMs = deps.waitMs ?? 500;
  let tools: string[] = [];
  for (let i = 0; i < attempts; i += 1) {
    tools = connectionTools(await list(), connectionId);
    if (tools.length > 0) break;
    if (i + 1 < attempts) await new Promise((resolve) => setTimeout(resolve, waitMs));
  }
  if (tools.length === 0) return 0;
  await grant(connectionId, [...agents], tools);
  return tools.length;
}

/** What the address form holds. */
export interface AddressForm {
  name: string;
  url: string;
  /** The key the service asked for, empty when it asked none. */
  key: string;
  /** Developer mode only: the transport, the local command, its headers or variables. */
  transport: "stdio" | "streamable-http" | "sse";
  command: string;
  args: string;
  headers: string;
}

export const EMPTY_ADDRESS_FORM: AddressForm = {
  name: "",
  url: "",
  key: "",
  transport: "streamable-http",
  command: "",
  args: "",
  headers: "",
};

/** The header a key typed in the address form is sent under. */
export const KEY_HEADER = "Authorization";

/** The name a service reached at `url` is kept under when none is typed. */
export function nameFromAddress(url: string): string {
  try {
    return sanitizeServerName(new URL(url.trim()).hostname.replace(/^www\./, ""));
  } catch {
    return "custom-mcp";
  }
}

/** `NAME=value` lines, comments and blanks left out. */
export function parsePairs(text: string): Record<string, string> {
  const out: Record<string, string> = {};
  for (const line of text.split(/\r?\n/)) {
    const trimmed = line.trim();
    if (!trimmed || trimmed.startsWith("#")) continue;
    const eq = trimmed.indexOf("=");
    if (eq <= 0) continue;
    out[trimmed.slice(0, eq).trim()] = trimmed.slice(eq + 1).trim();
  }
  return out;
}

export type AddressConfig =
  | { kind: "ok"; config: McpServerConfigInput }
  | { kind: "missing"; field: "url" | "command" };

/**
 * The configuration of a service by its address. Outside developer mode it is
 * reached at an address, and a key, when the service asked one, travels as
 * its authorization header, which the runtime files in the secret store; the
 * transport, a local command, headers and variables are the developer mode's.
 */
export function addressConfig(form: AddressForm, developerMode: boolean): AddressConfig {
  const transport = developerMode ? form.transport : "streamable-http";
  const env: Record<string, string> = developerMode ? parsePairs(form.headers) : {};
  const key = form.key.trim();
  if (key.length > 0 && transport !== "stdio") {
    // i18n-ignore: HTTP authorization scheme, never shown
    env[KEY_HEADER] = `Bearer ${key}`;
  }
  if (transport === "stdio") {
    if (!form.command.trim()) return { kind: "missing", field: "command" };
    return {
      kind: "ok",
      config: {
        name: sanitizeServerName(form.name.trim() || form.command.trim()),
        command: form.command.trim(),
        args: form.args.split(/\s+/).filter((a) => a.length > 0),
        env,
        transport,
        tags: [],
      },
    };
  }
  if (!form.url.trim()) return { kind: "missing", field: "url" };
  return {
    kind: "ok",
    config: {
      name: form.name.trim() ? sanitizeServerName(form.name) : nameFromAddress(form.url),
      url: form.url.trim(),
      env,
      transport,
      tags: [],
    },
  };
}

/**
 * What the flow keeps of a server once it is added: its name and transport,
 * which the trial and the removal read, and none of its variables or headers.
 * A key typed in the address form lives in the flow only until the runtime
 * holds it; before that, the whole configuration is kept, so a refused
 * attempt can be tried again with the same key.
 */
export function keptOnceAdded(config: McpServerConfigInput): McpServerConfigInput {
  return { ...config, env: {} };
}

/** Whether a test of the address answered that the service wants a key. */
export function asksForKey(messageKey: string): boolean {
  return (
    messageKey === "integrations.wizard.test_err.unauthorized" ||
    messageKey === "integrations.wizard.test_err.forbidden"
  );
}

/** A field an entry of the catalogue declares: a variable or a header. */
export interface DeclaredField {
  name: string;
  description?: string | null;
  isSecret: boolean;
}

/**
 * The label of a declared field. Its name (`BRAVE_API_KEY`, `Authorization`)
 * is the developer mode's; otherwise a secret reads as a key and any other
 * value by its description.
 */
export function fieldLabel(
  field: DeclaredField,
  developerMode: boolean,
  words: { key: string; value: string },
): string {
  if (developerMode) return field.name;
  if (field.isSecret) return words.key;
  const description = field.description?.trim();
  return description && description.length > 0 ? description : words.value;
}

/**
 * The permissions a sign-in asks, as the operator reads them: the descriptions
 * the service gives. The identifiers of the scopes are the developer mode's.
 */
export function plainScopes(descriptions: Record<string, string>, scopes: readonly string[]): string[] {
  return scopes.map((scope) => descriptions[scope]?.trim() ?? "").filter((d) => d.length > 0);
}
