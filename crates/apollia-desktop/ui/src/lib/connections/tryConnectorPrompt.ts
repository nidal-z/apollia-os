/**
 * "Try" on a connection, whatever its kind: a native account, a server, a
 * folder, a mailbox.
 *
 * The operator picks a question, one per tool when the selection entry
 * carries examples, a general question about what the connection allows
 * otherwise, or types their own. Trying it opens a conversation with Apollia
 * limited to the tools of that connection, read from the connection list,
 * sends the question, and routes to that conversation. The limit is asked of
 * the runtime when the conversation is created: nothing else Apollia holds is
 * offered there, and a tool of the connection Apollia does not hold asks
 * before each call. Nothing is sent, and
 * the operator stays where they were, when the connection is gone, exposes no
 * tool, or the session cannot be created: they are told why instead of
 * landing on an empty chat.
 *
 * The decision lives in this module rather than in the route because the
 * frontend runner mounts no Svelte component, so a policy left in the route is
 * out of reach of every guard.
 */
import type { ConnectionListState } from "$lib/connections/connectionList";
import type {
  ChatSessionSummary,
  Connection,
  CreateSessionRequest,
  TryExample,
} from "$lib/types";

export interface TryConnectorPromptDeps {
  /** The connection list, by its loader. */
  loadConnections: () => Promise<ConnectionListState>;
  /** Creates the conversation `request` describes. Rejects when the backend refused it. */
  createSession: (request: CreateSessionRequest) => Promise<ChatSessionSummary>;
  /** Sends the prompt as the opening message of the session. */
  sendMessage: (sessionId: string, prompt: string) => Promise<unknown>;
  /** Starts naming the session from its opening prompt. */
  nameSession: (sessionId: string, prompt: string) => void;
  /** Hands the session id to the chat route. */
  rememberSession: (sessionId: string) => void;
  /** Surfaces the failure where the operator is looking. */
  report: (err: unknown) => void;
  /** Routes to the chat screen. Reached only once a session exists. */
  navigate: () => void;
  /** Localised text reported when the connection exposes no tool. */
  noToolsMessage: string;
  /** Localised text reported when the connection is no longer listed. */
  notFoundMessage: string;
}

/** Identifiers of the tools of `connectionId`, as the connection list carries them. */
export async function connectionToolIds(
  connectionId: string,
  loadConnections: TryConnectorPromptDeps["loadConnections"],
): Promise<string[] | null> {
  const state = await loadConnections();
  if (state.status === "error") throw new Error(state.message);
  if (state.status !== "ready") return null;
  const connection = state.connections.find((c) => c.id === connectionId);
  return connection ? connection.tools.map((t) => t.id) : null;
}

/**
 * Returns `true` when the session exists and the route changed, `false` when
 * the failure was reported and the operator stayed where they were.
 */
export async function tryConnectorPrompt(
  connectionId: string,
  prompt: string,
  deps: TryConnectorPromptDeps,
): Promise<boolean> {
  let session: ChatSessionSummary;
  try {
    const tools = await connectionToolIds(connectionId, deps.loadConnections);
    if (tools === null) {
      deps.report(new Error(deps.notFoundMessage));
      return false;
    }
    if (tools.length === 0) {
      deps.report(new Error(deps.noToolsMessage));
      return false;
    }
    session = await deps.createSession({ mode: "libre", tools, limited_to_tools: true });
  } catch (err) {
    deps.report(err);
    return false;
  }
  deps.nameSession(session.id, prompt);
  // A failed send leaves an empty session the operator can type into; it must
  // not block the route change.
  await deps.sendMessage(session.id, prompt).catch(() => {});
  deps.rememberSession(session.id);
  deps.navigate();
  return true;
}

/** One question the "Try" dialog offers. */
export interface TryQuestion {
  id: string;
  /** What is tried: the tool label ("search messages"), or "General question". */
  label: string;
  question: string;
  /** The general question, offered when the connection carries no example. */
  general: boolean;
}

/** Texts of the general question, already localised. */
export interface GeneralQuestion {
  label: string;
  question: string;
}

function inLocale(texts: Record<string, string>, locale: string): string {
  const language = locale.split("-")[0] ?? locale;
  return texts[language] ?? texts.en ?? Object.values(texts)[0] ?? "";
}

/** Whether `connection` publishes the server tool `tool`; any tool while none is listed yet. */
function offers(connection: Connection, tool: string): boolean {
  if (connection.tools.length === 0) return true;
  return connection.tools.some((t) => t.id === tool || t.id.endsWith(`/${tool}`));
}

/**
 * The questions offered to try `connection`: one per example of its selection
 * entry whose tool the connection publishes, or the general question when
 * none is left.
 */
export function tryQuestions(
  connection: Connection,
  examples: readonly TryExample[],
  locale: string,
  general: GeneralQuestion,
): TryQuestion[] {
  const offered = examples
    .filter((e) => offers(connection, e.tool))
    .map((e) => ({
      id: e.tool,
      label: inLocale(e.label, locale),
      question: inLocale(e.question, locale),
      general: false,
    }))
    .filter((q) => q.question.trim().length > 0);
  if (offered.length > 0) return offered;
  return [{ id: "general", label: general.label, question: general.question, general: true }];
}
