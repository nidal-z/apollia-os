/**
 * The lease an open conversation holds on its local model.
 *
 * While a conversation is open in a pane, the runtime keeps the model it
 * talks to in memory; once the last lease on a model is released, the
 * runtime unloads it unless it is pinned. The lease follows the pane, not
 * the screen: it is released when the conversation is closed, when another
 * one takes its pane, or when the conversation changes model. Leaving the
 * chat for another screen releases nothing.
 *
 * Calls are queued, so a release always reaches the runtime before the open
 * that follows it. A lease is best effort: a host without a local engine
 * refuses it, and the conversation works all the same.
 */
import { openModelLease, releaseModelLease } from "$lib/ipc/llmLoad";
import {
  conversationModel,
  type ConversationTarget,
  type TargetGroups,
} from "$lib/chat/chatTargets";

/** The pane of the chat, while it shows one conversation at a time. */
export const MAIN_PANE = "main";

/** What a pane holds a lease for. */
export interface HeldLease {
  conversationId: string;
  modelPath: string;
}

/** The two calls a keeper makes, replaceable in a test. */
export interface LeaseCalls {
  open: (conversationId: string, modelPath: string) => Promise<unknown>;
  release: (conversationId: string) => Promise<unknown>;
}

export interface LeaseKeeper {
  /**
   * The pane shows `conversationId`, which talks to the local model at
   * `modelPath`; `null` for a conversation on a model of a server.
   */
  hold: (pane: string, conversationId: string, modelPath: string | null) => Promise<void>;
  /** The conversation of the pane was closed. */
  drop: (pane: string) => Promise<void>;
  /** What the pane holds a lease for, `null` when nothing. */
  held: (pane: string) => HeldLease | null;
}

/** A keeper of the leases of the panes, over `calls`. */
export function createLeaseKeeper(calls: LeaseCalls): LeaseKeeper {
  const leases = new Map<string, HeldLease>();
  let queue: Promise<void> = Promise.resolve();

  function enqueue(call: () => Promise<unknown>): Promise<void> {
    queue = queue.then(call).then(
      () => undefined,
      () => undefined,
    );
    return queue;
  }

  function release(pane: string): void {
    const current = leases.get(pane);
    if (!current) return;
    leases.delete(pane);
    void enqueue(() => calls.release(current.conversationId));
  }

  return {
    hold(pane, conversationId, modelPath) {
      const current = leases.get(pane) ?? null;
      if (
        current !== null &&
        current.conversationId === conversationId &&
        current.modelPath === modelPath
      ) {
        return queue;
      }
      release(pane);
      if (modelPath !== null) {
        leases.set(pane, { conversationId, modelPath });
        void enqueue(() => calls.open(conversationId, modelPath));
      }
      return queue;
    },
    drop(pane) {
      release(pane);
      return queue;
    },
    held: (pane) => leases.get(pane) ?? null,
  };
}

/** The leases of this application, held through the runtime. */
export const paneLeases = createLeaseKeeper({
  open: openModelLease,
  release: releaseModelLease,
});

/** What the lease of a conversation is read from. */
export interface LeaseReading {
  /** Whether the agents and the models were listed at least once. */
  loaded: boolean;
  groups: TargetGroups;
  defaultModel: string | null;
}

/**
 * Holds the lease of the conversation a pane shows, on the local model it
 * talks to. Nothing moves until both the conversation and the list of models
 * are known: a lease is never released on a guess.
 */
export function syncConversationLease(
  keeper: LeaseKeeper,
  pane: string,
  conversationId: string,
  conversation: ConversationTarget | null,
  reading: LeaseReading,
): Promise<void> | null {
  if (conversation === null || !reading.loaded) return null;
  const model = conversationModel(reading.groups, conversation, reading.defaultModel);
  return keeper.hold(pane, conversationId, model?.path ?? null);
}
