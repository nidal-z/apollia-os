/**
 * Adding a connection from the selection, after its warning was accepted.
 *
 * The add dialog asks for two gestures: choose an entry, accept its warning.
 * What follows runs on its own and is held here, outside the component, so a
 * test can play it against fake commands: the secrets typed in the
 * authentication step are filed, the server is added with the acceptance
 * journaled, Node.js is downloaded first when the server needs it (its
 * progress read while the addition waits), and the connection is tried once
 * added. An admission refusal is told apart from a failure, by the reason the
 * runtime gives, so the dialog can say who refused.
 */
import { invoke } from "@tauri-apps/api/core";
import { testOutcomeFromHealth } from "$lib/connections/status";
import {
  installConnector,
  nodeRuntimeState,
  type NodeRuntimeState,
  type PendingSecret,
  type WizardInvoke,
} from "$lib/ipc/connections";
import type { McpConnectionTestResponse, McpServerConfigInput } from "$lib/types";

export type { NodeRuntimeState };

/** The refusals the connection admission gives, by code. */
export type RefusalCode = "remote_denied" | "policy_unreadable";

/** Who a refusal comes from: the policy of the organisation, or the person's own permissions. */
export type RefusalAuthority = "organisation" | "permissions";

export interface Refusal {
  code: RefusalCode;
  by: RefusalAuthority;
  /** The reason as the runtime gave it. */
  reason: string;
}

/**
 * The reason the admission gives when the organisation policy denies online
 * connections. It is the runtime's `REMOTE_DENIED_REASON`, verbatim.
 */
export const REMOTE_DENIED_REASON =
  // i18n-ignore: runtime reason matched verbatim, never shown to the user
  "refused by the organisation policy: connections to online services are not allowed";

/** Start of the reason the admission gives when the deposited policy cannot be read. */
// i18n-ignore: runtime reason prefix matched verbatim, never shown to the user
const POLICY_UNREADABLE_PREFIX = "the connection policy could not be read";

/**
 * The texts of each refusal, by code: the add dialog shows them, and the
 * Journal names a refusal with the same keys.
 */
export const REFUSAL_TEXT_KEYS: Record<RefusalCode, { title: string; body: string }> = {
  remote_denied: {
    title: "connections.refusal.organisation_title",
    body: "connections.refusal.remote_denied",
  },
  policy_unreadable: {
    title: "connections.refusal.organisation_title",
    body: "connections.refusal.policy_unreadable",
  },
};

/** The refusal an error message carries, or `null` when it is another failure. */
export function refusalOf(message: string): Refusal | null {
  if (message === REMOTE_DENIED_REASON) {
    return { code: "remote_denied", by: "organisation", reason: message };
  }
  if (message.startsWith(POLICY_UNREADABLE_PREFIX)) {
    return { code: "policy_unreadable", by: "organisation", reason: message };
  }
  return null;
}

/** Whether adding `config` downloads Node.js first: an `npx` server runs on it. */
export function needsNode(config: McpServerConfigInput): boolean {
  return config.command === "npx";
}

/** Share of the Node.js archive received, 0 to 100, or `null` while its size is unknown. */
export function nodePercent(state: NodeRuntimeState): number | null {
  if (state.state !== "downloading" || state.total <= 0) return null;
  return Math.min(100, Math.round((state.received / state.total) * 100));
}

export type AdditionOutcome =
  | { kind: "added" }
  | { kind: "refused"; refusal: Refusal }
  | { kind: "failed"; message: string };

export interface AdditionOptions {
  invokeFn?: WizardInvoke;
  /** Called with each state of Node.js read while the addition waits. */
  onNode?: (state: NodeRuntimeState) => void;
  /** How the state of Node.js is read; the Tauri command by default. */
  readNode?: () => Promise<NodeRuntimeState>;
  /** Interval between two reads of the Node.js state. */
  pollMs?: number;
}

function messageOf(err: unknown): string {
  if (err instanceof Error) return err.message;
  if (typeof err === "string") return err;
  return String(err);
}

/**
 * File the secrets, then add the server with its warning accepted. While the
 * addition waits on a server that needs Node.js, the state of its download is
 * read every `pollMs` and handed to `onNode`.
 */
export async function addConnection(
  config: McpServerConfigInput,
  secrets: readonly PendingSecret[],
  options: AdditionOptions = {},
): Promise<AdditionOutcome> {
  const invokeFn = options.invokeFn ?? invoke;
  let timer: ReturnType<typeof setInterval> | null = null;
  if (needsNode(config) && options.onNode) {
    const onNode = options.onNode;
    const readNode = options.readNode ?? nodeRuntimeState;
    const read = (): void => {
      void readNode()
        .then(onNode)
        .catch(() => {});
    };
    read();
    timer = setInterval(read, options.pollMs ?? 500);
  }
  try {
    await installConnector(config, secrets, invokeFn);
    return { kind: "added" };
  } catch (err: unknown) {
    const message = messageOf(err);
    const refusal = refusalOf(message);
    return refusal ? { kind: "refused", refusal } : { kind: "failed", message };
  } finally {
    if (timer !== null) clearInterval(timer);
  }
}

export type TrialOutcome =
  | { kind: "succeeded"; tools: number }
  | { kind: "failed"; messageKey: string; detail: string | null };

/**
 * Try the connection `name` once it is added: the live session is tested and
 * its health read. A reachable server whose real calls fail is a failure.
 */
export async function tryAddedConnection(
  name: string,
  invokeFn: WizardInvoke = invoke,
): Promise<TrialOutcome> {
  let response: McpConnectionTestResponse;
  try {
    response = await invokeFn<McpConnectionTestResponse>("test_mcp_live_server", { name });
  } catch (err: unknown) {
    const detail = messageOf(err);
    return { kind: "failed", messageKey: testErrorKey(detail), detail };
  }
  if (response.kind === "oauth_required") {
    return {
      kind: "failed",
      messageKey: "integrations.wizard.test_err.oauth_required",
      detail: null,
    };
  }
  const outcome = testOutcomeFromHealth(response.live_health ?? null);
  if (outcome.verdict === "ok" || outcome.verdict === "unverified") {
    return { kind: "succeeded", tools: response.tools.length };
  }
  const detail = outcome.error ?? outcome.detail ?? null;
  const messageKey =
    outcome.verdict === "needs_reauth"
      ? "integrations.wizard.test_err.oauth_required"
      : detail
        ? testErrorKey(detail)
        : "integrations.wizard.test_err.generic";
  return { kind: "failed", messageKey, detail };
}

/** The message, in plain words, for a raw error of a connection test. */
export function testErrorKey(raw: string): string {
  const lower = raw.toLowerCase();
  if (lower.includes("enoent") || lower.includes("command not found")) {
    return "integrations.wizard.test_err.command_missing";
  }
  if (lower.includes("401") || lower.includes("unauthorized")) {
    return "integrations.wizard.test_err.unauthorized";
  }
  if (lower.includes("403") || lower.includes("forbidden")) {
    return "integrations.wizard.test_err.forbidden";
  }
  if (lower.includes("404") || lower.includes("not found")) {
    return "integrations.wizard.test_err.not_found";
  }
  if (lower.includes("timeout") || lower.includes("timed out")) {
    // The server started but never answered the handshake: a wrong endpoint,
    // a slow first download, or an authentication that takes too long.
    return "integrations.wizard.test_err.handshake_timeout";
  }
  if (
    lower.includes("transport closed") ||
    lower.includes("transport send channel closed") ||
    lower.includes("server exited") ||
    lower.includes("stdin closed")
  ) {
    return "integrations.wizard.test_err.transport_closed";
  }
  if (lower.includes("network") || lower.includes("dns") || lower.includes("connect")) {
    return "integrations.wizard.test_err.network";
  }
  return "integrations.wizard.test_err.generic";
}
