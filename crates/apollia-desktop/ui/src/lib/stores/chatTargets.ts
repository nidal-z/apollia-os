/**
 * The agents and the models the chat can talk to, read once and shared: the
 * selector lists them, and the conversation reads from them which local
 * model it holds in memory.
 */
import { writable } from "svelte/store";
import { listChatTargets } from "$lib/ipc/chatTargets";
import { buildTargetGroups, type TargetGroups } from "$lib/chat/chatTargets";

export interface ChatTargetsState {
  /** Whether the list was read at least once. */
  loaded: boolean;
  groups: TargetGroups;
  /** The default model, by the name the router routes by. */
  defaultModel: string | null;
  /** The servers that did not answer. */
  unreachable: string[];
}

export const chatTargets = writable<ChatTargetsState>({
  loaded: false,
  groups: { agents: [], models: [] },
  defaultModel: null,
  unreachable: [],
});

/** Reads the list again; a read that fails leaves what was known. */
export async function refreshChatTargets(): Promise<void> {
  try {
    const payload = await listChatTargets();
    const fallback = payload.default_model?.model_id;
    chatTargets.set({
      loaded: true,
      groups: buildTargetGroups(payload),
      defaultModel: typeof fallback === "string" ? fallback : null,
      unreachable: payload.servers.filter((s) => s.error !== null).map((s) => s.server),
    });
  } catch {
    // The list stays as it was; the selector still shows the conversation.
  }
}
