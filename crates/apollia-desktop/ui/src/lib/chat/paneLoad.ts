/**
 * What a pane of a divided chat does before it loads a model of this
 * computer: it asks what the load would take, beside the models the other
 * panes already hold in memory.
 *
 * A load the guardrail refuses is not sent and the loading window does not
 * open: the refusal is kept for that pane alone, which shows it in place.
 * From there the user picks a model already in memory, closes a pane, or
 * opens the loading window, where a load beyond the limit asks for an
 * explicit confirmation.
 */
import { writable, type Readable } from "svelte/store";
import type { ModelTarget } from "$lib/chat/chatTargets";
import { refusalOf, type LoadRefusal } from "$lib/chat/modelLoad";
import type { LoadEstimate, ModelLoadRequest } from "$lib/types";

/** The load a pane was refused. */
export interface PaneRefusal {
  /** The model the pane asked for. */
  target: ModelTarget;
  refusal: LoadRefusal;
}

export interface PaneLoads {
  /** The refusal each pane shows, by pane identifier. */
  refusals: Readable<Record<string, PaneRefusal>>;
  /**
   * Asks whether `pane` may load `target`. `true`: the loading window opens
   * as usual. `false`: the guardrail refuses, and the pane holds the refusal.
   */
  gate: (pane: string, target: ModelTarget) => Promise<boolean>;
  /** The pane shows its refusal no more. */
  dismiss: (pane: string) => void;
}

/** The refusals of the panes, over the call that estimates a load. */
export function createPaneLoads(
  estimate: (request: ModelLoadRequest) => Promise<LoadEstimate>,
): PaneLoads {
  const refusals = writable<Record<string, PaneRefusal>>({});

  function dismiss(pane: string): void {
    refusals.update((all) => {
      if (!(pane in all)) return all;
      const next = { ...all };
      delete next[pane];
      return next;
    });
  }

  return {
    refusals: { subscribe: refusals.subscribe },
    async gate(pane, target) {
      dismiss(pane);
      let answer: LoadEstimate;
      try {
        answer = await estimate({ modelId: target.route });
      } catch {
        // The loading window estimates again and says what went wrong.
        return true;
      }
      const refusal = refusalOf(answer);
      if (refusal === null) return true;
      refusals.update((all) => ({ ...all, [pane]: { target, refusal } }));
      return false;
    },
    dismiss,
  };
}
