/**
 * The agent the first launch ended up with, and what the steps after the
 * "first agent" step read from it.
 *
 * The step offers three ways to an agent: install a package file, create one
 * by describing the need, or start from an example. Whichever way is taken,
 * the agent obtained is kept in `onboardingFirstAgent`
 * (`$lib/stores/onboarding`). A step that is passed keeps nothing, and the
 * functions below say what follows from that: no connection to propose, and
 * a first conversation with Apollia.
 */
import { APOLLIA_AGENT_ID } from "$lib/chat/chatTargets";

/** The three ways to a first agent. */
export type FirstAgentWay = "install" | "create" | "example";

const WAYS: readonly FirstAgentWay[] = ["install", "create", "example"];

/** The agent kept for the steps that follow. */
export interface FirstAgent {
  /** Identifier of the agent: what a conversation is opened with. */
  id: string;
  /** Name shown. */
  name: string;
  /** The way it was obtained. */
  way: FirstAgentWay;
  /** The package that brought it; `null` for an agent created in the step. */
  packageName: string | null;
}

/**
 * Reads back an agent that was stored as text. Anything that is not one
 * answers `null`: a stored value this interface does not understand is an
 * onboarding without an agent, never an error.
 */
export function parseFirstAgent(raw: string | null): FirstAgent | null {
  if (!raw) return null;
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return null;
  }
  if (typeof value !== "object" || value === null) return null;
  const { id, name, way, packageName } = value as Record<string, unknown>;
  if (typeof id !== "string" || id.length === 0) return null;
  if (typeof name !== "string") return null;
  if (!WAYS.includes(way as FirstAgentWay)) return null;
  if (packageName !== null && typeof packageName !== "string") return null;
  return { id, name: name || id, way: way as FirstAgentWay, packageName };
}

/**
 * Whether the connection step has anything to propose. Without an agent it
 * has not: the connections it proposes are those the chosen agent requires.
 * An agent created in the step comes from no package and requires none.
 */
export function proposesConnections(agent: FirstAgent | null): boolean {
  return agent !== null && agent.packageName !== null;
}

/** The package whose required connections the connection step reads. */
export function connectionsPackage(agent: FirstAgent | null): string | null {
  return agent?.packageName ?? null;
}

/**
 * The agent the first task is done with: the one kept, else Apollia, the
 * agent every installation has.
 */
export function firstTaskAgentId(agent: FirstAgent | null): string {
  return agent?.id ?? APOLLIA_AGENT_ID;
}
