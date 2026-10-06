/**
 * The selector of a conversation: the agents and the models the chat can
 * talk to, the search over both, and what a choice does.
 *
 * Pure functions over the payload of `list_chat_targets`, so the selector
 * component only renders and the rules are tested on their own.
 */

/** The identifier of the built-in agent a model is chosen to talk to. */
export const APOLLIA_AGENT_ID = "apollia:chat";

/** Storage format of a local model, shown as a badge. */
export type ModelFormat = "gguf" | "mlx";

/** An agent the selector offers, with the model it runs on. */
export interface AgentTarget {
  kind: "agent";
  /** Agent identifier, the name a session is created with. */
  id: string;
  /** Display name. */
  name: string;
  /** The model resolved for the agent, `null` for the default model. */
  model: string | null;
  /** True for the built-in agent, reached by a free chat. */
  builtin: boolean;
}

/** A model the selector offers, to talk to the built-in agent with. */
export interface ModelTarget {
  kind: "model";
  /** What the session is created with: the index identifier of a local
   * model, or `<server>/<model>`. */
  route: string;
  /** Display name. */
  label: string;
  /** Where the model runs. */
  source: "local" | "server";
  /** Storage format of a local model, `null` for a model of a server. */
  format: ModelFormat | null;
  /** The server of a served model, `null` for a local one. */
  server: string | null;
  /** Parameter count label of a local model, e.g. `"8B"`. */
  parameters: string | null;
}

export type ChatTarget = AgentTarget | ModelTarget;

/** The selector's two groups, in their display order. */
export interface TargetGroups {
  agents: AgentTarget[];
  models: ModelTarget[];
}

/** Payload of `list_chat_targets`, as loosely typed as the routes it reads. */
export interface ChatTargetsPayload {
  agents: Array<Record<string, unknown>>;
  installed: Array<Record<string, unknown>>;
  servers: Array<{
    server: string;
    models: Array<Record<string, unknown>>;
    error: string | null;
  }>;
  default_model: Record<string, unknown> | null;
}

function text(value: unknown): string | null {
  return typeof value === "string" && value.trim() !== "" ? value : null;
}

function agentModel(agent: Record<string, unknown>): string | null {
  const model = agent.model;
  if (model && typeof model === "object") {
    return text((model as Record<string, unknown>).model_id);
  }
  return null;
}

/**
 * The selector's groups from the payload: the enabled agents, the built-in
 * one first; then the language models of the machine, then those of each
 * server. A model of any other category (embeddings, dictation, draft) is
 * never offered: the chat cannot talk to it.
 */
export function buildTargetGroups(payload: ChatTargetsPayload): TargetGroups {
  const agents: AgentTarget[] = [];
  for (const raw of payload.agents) {
    const id = text(raw.id);
    if (!id || raw.enabled === false) continue;
    agents.push({
      kind: "agent",
      id,
      name: text(raw.name) ?? id,
      model: agentModel(raw),
      builtin: id === APOLLIA_AGENT_ID,
    });
  }
  agents.sort((a, b) =>
    a.builtin === b.builtin ? a.name.localeCompare(b.name) : a.builtin ? -1 : 1,
  );

  const models: ModelTarget[] = [];
  for (const raw of payload.installed) {
    const id = text(raw.id);
    if (!id || raw.category !== "language") continue;
    const format = raw.format === "mlx" || raw.format === "gguf" ? raw.format : null;
    models.push({
      kind: "model",
      route: id,
      label: id,
      source: "local",
      format,
      server: null,
      parameters: text(raw.parameters),
    });
  }
  for (const server of payload.servers) {
    for (const raw of server.models) {
      const route = text(raw.route);
      const id = text(raw.id);
      if (!route || !id) continue;
      models.push({
        kind: "model",
        route,
        label: id,
        source: "server",
        format: null,
        server: server.server,
        parameters: null,
      });
    }
  }
  return { agents, models };
}

/** The groups narrowed to what matches `query`, case and accents ignored. */
export function filterTargetGroups(groups: TargetGroups, query: string): TargetGroups {
  const needle = fold(query.trim());
  if (needle === "") return groups;
  const hit = (...fields: Array<string | null>) =>
    fields.some((field) => field !== null && fold(field).includes(needle));
  return {
    agents: groups.agents.filter((a) => hit(a.name, a.id, a.model)),
    models: groups.models.filter((m) => hit(m.label, m.route, m.server, m.format)),
  };
}

function fold(value: string): string {
  return value
    .normalize("NFD")
    .replace(/\p{Diacritic}/gu, "")
    .toLowerCase();
}

/**
 * The backend row name the runtime gives a local model chosen by its index
 * identifier: lowercase, every other character than a letter, a digit, `_`
 * and `-` replaced by `-`. Mirrors `local_engine::row_name`.
 */
export function rowName(indexId: string): string {
  return Array.from(indexId.toLowerCase())
    .map((c) => (/^[a-z0-9_-]$/.test(c) ? c : "-"))
    .join("");
}

/**
 * The display name of the model a session names: the label of the model
 * whose route or row is `name`, else `name` itself.
 */
export function modelLabel(groups: TargetGroups, name: string | null): string | null {
  if (!name) return null;
  const found = groups.models.find((m) => m.route === name || rowName(m.route) === name);
  return found ? found.label : name;
}

/** What the closed selector shows for a conversation. */
export interface TargetLabel {
  /** The agent, `null` when the conversation has no target yet. */
  agent: string | null;
  /** The model, `null` when it is not known. */
  model: string | null;
}

/** The conversation the selector speaks for. */
export interface ConversationTarget {
  mode: "libre" | "agent";
  agentName: string | null;
  /** The model chosen for the session, by the name the router routes by. */
  model: string | null;
}

/**
 * The label of the closed selector: the agent and its model, or Apollia and
 * the model; `null` for a conversation with no target yet.
 */
export function targetLabel(
  groups: TargetGroups,
  conversation: ConversationTarget | null,
  defaultModel: string | null,
  apolliaName: string,
): TargetLabel | null {
  if (!conversation) return null;
  const agentId = conversation.mode === "agent" ? conversation.agentName : APOLLIA_AGENT_ID;
  const agent = groups.agents.find((a) => a.id === agentId) ?? null;
  const name = conversation.mode === "agent" ? (agent?.name ?? agentId) : apolliaName;
  const model = conversation.model ?? agent?.model ?? defaultModel;
  return { agent: name, model: modelLabel(groups, model) };
}

/** What choosing a target does to the conversation it is chosen on. */
export type TargetAction =
  /** The model becomes the model of this conversation. */
  | { kind: "set_model"; route: string }
  /** A new conversation opens on the target; `replace` removes the empty
   * conversation it was chosen on, so the choice reads as made in place. */
  | {
      kind: "new_session";
      mode: "libre" | "agent";
      agentName: string | null;
      model: string | null;
      replace: boolean;
    };

/**
 * What choosing `target` does on `conversation`, as the maintainer settled
 * it: a model becomes the model of the conversation, agent kept, and on an
 * empty conversation of another agent it starts the conversation with
 * Apollia on that model; an agent opens a new conversation with it, in place
 * of an empty one.
 */
export function targetAction(
  target: ChatTarget,
  conversation: ConversationTarget,
  messageCount: number,
): TargetAction {
  const empty = messageCount === 0;
  if (target.kind === "model") {
    if (conversation.mode === "agent" && empty) {
      return { kind: "new_session", mode: "libre", agentName: null, model: target.route, replace: true };
    }
    return { kind: "set_model", route: target.route };
  }
  if (target.builtin) {
    return { kind: "new_session", mode: "libre", agentName: null, model: null, replace: empty };
  }
  return { kind: "new_session", mode: "agent", agentName: target.id, model: null, replace: empty };
}
