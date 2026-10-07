/**
 * The live accesses of the conversation the Integrations tab shows.
 *
 * The tab reads the agent in force, what the machine offers, and the
 * settings of the conversation. A switch writes at once: the settings are
 * replaced on the runtime and read by the next message. A refused write puts
 * the switch back and says why. Neither the agent nor the permissions are
 * written.
 */
import type { SessionSettings } from "$lib/chat/sessionSettings";
import {
  agentProfileSwitch,
  sessionAccessRows,
  switchSessionAccess,
  type NativeWords,
  type SessionAccessRow,
  type TargetOffer,
  type ToolOffer,
} from "$lib/chat/sessionAccesses";

/** The part of the agent in force the tab reads. */
export interface AgentInForce {
  access: string[];
  sources: string[];
}

/** What the accesses of a conversation need from the runtime. */
export interface SessionAccessesEffects {
  getSettings: (sessionId: string) => Promise<SessionSettings>;
  setSettings: (sessionId: string, settings: SessionSettings) => Promise<SessionSettings>;
  getAgent: (agentId: string) => Promise<AgentInForce | null>;
  listTools: () => Promise<ToolOffer[]>;
  listTargets: () => Promise<TargetOffer[]>;
}

export interface SessionAccessesController {
  readonly sessionId: string | null;
  readonly loading: boolean;
  /** The settings in force, as the last accepted write left them. */
  readonly settings: SessionSettings;
  /** Every line of the tab. */
  readonly rows: SessionAccessRow[];
  /** Whether the installed agent may read the profile in this conversation. */
  readonly agentReadsProfile: boolean;
  /** Why the last write was refused. */
  readonly error: string | null;
  load(sessionId: string, agentId: string): Promise<void>;
  switchAccess(row: SessionAccessRow, on: boolean): Promise<void>;
  setProfileAccess(on: boolean): Promise<void>;
  dismissError(): void;
}

function messageOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/** The controller of one Integrations tab. */
export function createSessionAccesses(
  effects: SessionAccessesEffects,
  native: ReadonlyMap<string, NativeWords> = new Map(),
): SessionAccessesController {
  let sessionId = $state<string | null>(null);
  let agentId = $state("");
  let loading = $state(false);
  let settings = $state.raw<SessionSettings>({});
  let agent = $state.raw<AgentInForce | null>(null);
  let tools = $state.raw<ToolOffer[]>([]);
  let targets = $state.raw<TargetOffer[]>([]);
  let error = $state<string | null>(null);
  // Writes leave in the order the switches were moved.
  let queue: Promise<void> = Promise.resolve();

  const rows = $derived(
    sessionAccessRows({ held: agent?.access ?? [], tools, targets, self: agentId, native }, settings),
  );
  const agentReadsProfile = $derived(agentProfileSwitch(settings, agent?.sources ?? []));

  function write(next: SessionSettings): Promise<void> {
    const id = sessionId;
    if (!id) return Promise.resolve();
    const previous = settings;
    settings = next;
    error = null;
    queue = queue.then(async () => {
      try {
        const stored = await effects.setSettings(id, next);
        if (sessionId === id && settings === next) settings = stored ?? next;
      } catch (e: unknown) {
        if (sessionId === id) {
          settings = previous;
          error = messageOf(e);
        }
      }
    });
    return queue;
  }

  return {
    get sessionId() { return sessionId; },
    get loading() { return loading; },
    get settings() { return settings; },
    get rows() { return rows; },
    get agentReadsProfile() { return agentReadsProfile; },
    get error() { return error; },

    async load(id: string, forAgent: string): Promise<void> {
      sessionId = id;
      agentId = forAgent;
      loading = true;
      error = null;
      settings = {};
      agent = null;
      const [read, inForce, listed, callable] = await Promise.allSettled([
        effects.getSettings(id),
        effects.getAgent(forAgent),
        effects.listTools(),
        effects.listTargets(),
      ]);
      if (sessionId !== id) return;
      if (read.status === "fulfilled") settings = read.value ?? {};
      else error = messageOf(read.reason);
      agent = inForce.status === "fulfilled" ? inForce.value : null;
      tools = listed.status === "fulfilled" ? (listed.value ?? []) : [];
      targets = callable.status === "fulfilled" ? (callable.value ?? []) : [];
      loading = false;
    },

    switchAccess(row: SessionAccessRow, on: boolean): Promise<void> {
      return write(switchSessionAccess(settings, row, on));
    },

    setProfileAccess(on: boolean): Promise<void> {
      return write({ ...settings, profile_access: on });
    },

    dismissError(): void {
      error = null;
    },
  };
}
