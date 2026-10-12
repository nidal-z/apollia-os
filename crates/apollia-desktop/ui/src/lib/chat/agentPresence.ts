/**
 * Whether the agent of a conversation is installed, as far as the interface
 * knows.
 *
 * The list of agents the interface holds is refreshed by events and can lag
 * the runtime by a moment: a conversation opened right after its agent was
 * installed finds a list that does not name it yet. Saying "no longer
 * installed" on that list alone was wrong for those seconds. So an agent absent
 * from the list is first checked against a fresh list, and is only called
 * missing once that fresh list was read and still does not name it.
 */

/** What is known of the agent of a conversation. */
export type AgentPresence =
  /** No agent to look for, or the list names it. */
  | "present"
  /** The list does not name it and a fresh list was not read yet. */
  | "checking"
  /** A fresh list was read and does not name it. */
  | "missing";

export interface AgentPresenceInput {
  mode: "libre" | "agent";
  agentName: string | null;
  /** Names of the agents the interface holds. */
  known: readonly string[];
  /** The agent a fresh list was last read for, `null` when none was. */
  verifiedFor: string | null;
}

export function agentPresence(input: AgentPresenceInput): AgentPresence {
  if (input.mode !== "agent" || input.agentName === null) return "present";
  if (input.known.includes(input.agentName)) return "present";
  return input.verifiedFor === input.agentName ? "missing" : "checking";
}
