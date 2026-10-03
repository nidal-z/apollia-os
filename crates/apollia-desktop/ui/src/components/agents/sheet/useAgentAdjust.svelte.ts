/**
 * The adjustment session of an agent's sheet.
 *
 * "Adjust" makes the whole sheet editable on one page with a single "Save".
 * The session holds the draft; "Cancel" drops the changes of the session and
 * nothing is written; "Save" sends one write per changed field, each recorded
 * by the runtime in the history of the agent; "Back to defaults" removes every
 * override saved for the agent, and its model, after a confirmation.
 *
 * Owned by the detail pane, which hands it to the header (its buttons) and to
 * the sheet (its editable sections). `revision` moves after every write so
 * the sheet reads the agent in force again.
 */
import { getAgentDefinition, type ResolvedAgent } from "$lib/ipc/agentSheet";
import {
  clearAgentModel,
  removeAgentOverride,
  resetAgentOverrides,
  setAgentModel,
  setAgentOverride,
} from "$lib/ipc/agentAdjust";
import {
  adjustmentWrites,
  changedFields,
  draftFrom,
  type AdjustDraft,
  type AdjustWrite,
  type MarkedField,
} from "./agentAdjust";

/** Runs one write of a save against the runtime. */
async function apply(agent: string, write: AdjustWrite): Promise<void> {
  switch (write.kind) {
    case "set_override":
      await setAgentOverride(agent, write.field, write.value);
      return;
    case "remove_override":
      await removeAgentOverride(agent, write.field);
      return;
    case "set_model":
      await setAgentModel(agent, write.model.model_source, write.model.model_id, write.model.preset_id);
      return;
    case "clear_model":
      await clearAgentModel(agent);
      return;
  }
}

export interface AgentAdjust {
  /** Whether the sheet is being adjusted. */
  readonly active: boolean;
  /** The agent in force when the session began. */
  readonly base: ResolvedAgent | null;
  /** What the user is changing; `null` outside a session. */
  readonly draft: AdjustDraft | null;
  /** The writes a save would send. */
  readonly writes: AdjustWrite[];
  /** The fields the session changes. */
  readonly changed: Set<MarkedField>;
  readonly saving: boolean;
  readonly resetting: boolean;
  /** Error of the last save or reset, as the runtime phrased it. */
  readonly error: string | null;
  /** Moves after every write, so the sheet reads the agent again. */
  readonly revision: number;
  begin(): Promise<void>;
  cancel(): void;
  save(): Promise<boolean>;
  reset(): Promise<boolean>;
}

/** A session for the agent `getId()` names; a change of agent ends it. */
export function createAgentAdjust(getId: () => string): AgentAdjust {
  let active = $state(false);
  let base = $state<ResolvedAgent | null>(null);
  let draft = $state<AdjustDraft | null>(null);
  let saving = $state(false);
  let resetting = $state(false);
  let error = $state<string | null>(null);
  let revision = $state(0);
  let sessionOf = "";

  const writes = $derived(base && draft ? adjustmentWrites(base, $state.snapshot(draft) as AdjustDraft) : []);
  const changed = $derived(
    base && draft ? changedFields(base, $state.snapshot(draft) as AdjustDraft) : new Set<MarkedField>(),
  );

  function end(): void {
    active = false;
    base = null;
    draft = null;
  }

  function message(err: unknown): string {
    return err instanceof Error ? err.message : String(err);
  }

  return {
    get active() {
      if (active && sessionOf !== getId()) return false;
      return active;
    },
    get base() {
      return base;
    },
    get draft() {
      return draft;
    },
    get writes() {
      return writes;
    },
    get changed() {
      return changed;
    },
    get saving() {
      return saving;
    },
    get resetting() {
      return resetting;
    },
    get error() {
      return error;
    },
    get revision() {
      return revision;
    },
    async begin() {
      const id = getId();
      error = null;
      const agent = await getAgentDefinition(id).catch((err: unknown) => {
        error = message(err);
        return null;
      });
      if (!agent || id !== getId()) return;
      sessionOf = id;
      base = agent;
      draft = draftFrom(agent);
      active = true;
    },
    cancel() {
      error = null;
      end();
    },
    async save() {
      if (!base || saving) return false;
      const id = sessionOf;
      const pending = [...writes];
      saving = true;
      error = null;
      try {
        for (const write of pending) await apply(id, write);
        end();
        return true;
      } catch (err) {
        error = message(err);
        return false;
      } finally {
        saving = false;
        revision += 1;
      }
    },
    async reset() {
      const id = getId();
      resetting = true;
      error = null;
      try {
        await resetAgentOverrides(id);
        end();
        return true;
      } catch (err) {
        error = message(err);
        return false;
      } finally {
        resetting = false;
        revision += 1;
      }
    },
  };
}
