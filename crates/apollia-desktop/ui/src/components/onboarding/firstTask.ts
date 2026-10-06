/**
 * The first task: what the first launch closes on.
 *
 * The modal does not end on an empty application. It closes on the agent at
 * work: a conversation with the agent kept at the "first agent" step, its
 * composer holding the first example the agent declares; for an example that
 * comes with a demonstration dataset, the copy of that dataset into a folder
 * the person chooses, then the launch of a batch on the copied workbook; and
 * without an agent, a conversation with Apollia, the agent every installation
 * has. Nothing is copied unasked: leaving the copy falls back on the
 * conversation.
 *
 * Kept free of Svelte and of the Tauri bridge so each decision is tested on
 * its own; the calls are handed in by `OnboardingFirstTask.svelte`.
 */
import { APOLLIA_AGENT_ID } from "$lib/chat/chatTargets";
import { firstTaskAgentId, type FirstAgent } from "$lib/onboarding/firstAgent";
import type { ChatSessionSummary, CreateSessionRequest } from "$lib/types";
import { hasDataset, type Trial } from "../agents/exampleTrial";

/** A conversation the first launch closes on. */
export interface FirstConversation {
  kind: "conversation";
  /** The agent the conversation is opened with. */
  agentId: string;
  /** Its name as shown; `null` for Apollia, which needs no introduction. */
  agentName: string | null;
}

/** The trial of an example on its demonstration dataset. */
export interface FirstTrial {
  kind: "trial";
  trial: Trial;
  /** The conversation opened instead when the copy is left. */
  fallback: FirstConversation;
}

export type FirstTaskPlan = FirstConversation | FirstTrial;

function conversationWith(agent: FirstAgent | null): FirstConversation {
  return {
    kind: "conversation",
    agentId: firstTaskAgentId(agent),
    agentName: agent?.name ?? null,
  };
}

/**
 * What the first launch closes on for the agent kept, `null` when the "first
 * agent" step was passed.
 */
export function firstTaskPlan(agent: FirstAgent | null): FirstTaskPlan {
  const conversation = conversationWith(agent);
  if (agent !== null && agent.packageName !== null && hasDataset(agent.packageName)) {
    return {
      kind: "trial",
      trial: { example: agent.packageName, agent: agent.id },
      fallback: conversation,
    };
  }
  return conversation;
}

/** The session a first conversation is created with. */
export function sessionRequestOf(conversation: FirstConversation): CreateSessionRequest {
  if (conversation.agentId === APOLLIA_AGENT_ID) return { mode: "libre" };
  return { mode: "agent", agent_name: conversation.agentId };
}

/**
 * The first example an agent declares, as its composer is prefilled with:
 * the first one that says something. `null` when it declares none.
 */
export function firstExampleOf(examples: readonly string[] | null | undefined): string | null {
  for (const example of examples ?? []) {
    const text = example.trim();
    if (text.length > 0) return text;
  }
  return null;
}

/** What the empty conversation says above the composer. */
export interface FirstTaskIntro {
  sessionId: string;
  agentName: string;
  /** Whether the composer holds the first example of the agent. */
  prefilled: boolean;
}

/** The calls a first conversation is opened with. */
export interface FirstConversationSteps {
  createSession: (request: CreateSessionRequest) => Promise<ChatSessionSummary>;
  /** The examples the agent declares; rejects or answers `null` without one. */
  agentExamples: (agentId: string) => Promise<readonly string[] | null>;
  /** Hands the session to the chat screen. */
  rememberSession: (sessionId: string) => void;
  /** Routes to the chat screen. Reached only once a session exists. */
  navigate: () => void | Promise<void>;
  /** Writes `text` in the composer of the conversation. */
  prefill: (text: string) => void;
  /** Says what the conversation is, above the composer. */
  introduce: (intro: FirstTaskIntro) => void;
  /** Surfaces a refused creation where the person is looking. */
  report: (err: unknown) => void;
}

/**
 * Opens the conversation the first launch closes on. With an agent kept, the
 * composer is prefilled with its first example and nothing is sent: the
 * person sends it as it is, or changes it. An agent whose examples cannot be
 * read still gets its conversation, with an empty composer.
 *
 * Returns `true` once the conversation is open, `false` when its creation was
 * refused and reported.
 */
export async function openFirstConversation(
  steps: FirstConversationSteps,
  conversation: FirstConversation,
): Promise<boolean> {
  let session: ChatSessionSummary;
  try {
    session = await steps.createSession(sessionRequestOf(conversation));
  } catch (err) {
    steps.report(err);
    return false;
  }
  steps.rememberSession(session.id);
  await steps.navigate();
  if (conversation.agentName === null) return true;
  let example: string | null = null;
  try {
    example = firstExampleOf(await steps.agentExamples(conversation.agentId));
  } catch {
    example = null;
  }
  if (example !== null) steps.prefill(example);
  steps.introduce({
    sessionId: session.id,
    agentName: conversation.agentName,
    prefilled: example !== null,
  });
  return true;
}
