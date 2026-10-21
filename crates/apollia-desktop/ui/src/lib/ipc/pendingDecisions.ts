// Typed IPC wrappers for the decisions waiting for the user: one list that
// gathers the consents of the conversations, the task pauses, the batch items
// and the questions of agents, and one call that answers an item of it.
//
// Failures are never swallowed: both wrappers reject with the runtime error
// string so the screen can humanize it (`reportError`).

import { invoke } from "@tauri-apps/api/core";
import type { AskUserAnswer } from "$lib/types";

/** Where a waiting decision comes from. */
export type PendingKind = "consent" | "task_pause" | "batch_item" | "question";

/** An answer a waiting decision takes. */
export type PendingAnswer = "once" | "always" | "refuse" | "answer";

/** Where the decision is found where it was born. */
export interface PendingOrigin {
  sessionId?: string;
  messageId?: string;
  toolCallId?: string;
  taskId?: string;
  batchId?: string;
  /** Position of a batch item in its source, from 0. */
  rank?: number;
  requestId?: string;
  /** The workflow run that asked a question. */
  runId?: string;
  /** Its workflow, `<package>/<workflow>`. */
  workflowId?: string;
  /** The step the run waits in. */
  step?: string;
}

/** One decision waiting for the user. */
export interface PendingDecision {
  /** Identifier to answer it by, stable while it waits. */
  id: string;
  kind: PendingKind;
  agentId: string | null;
  /** What is asked, in a sentence. */
  summary: string;
  /** The tool or the access asked for. */
  access: string | null;
  /** Its action family, when known. */
  family: string | null;
  /** The full arguments of the call, shown behind "see more". */
  detail?: string;
  /** The whole command or source, when the consent is about running code. */
  code?: string;
  /** The folder that code runs in. */
  workingDir?: string;
  answers: PendingAnswer[];
  /** The questions of a question, or the typed payload of a paused task. */
  questions: unknown;
  /** A free answer is taken besides the proposed choices. */
  allowOther: boolean;
  origin: PendingOrigin;
  /** When it was asked, RFC 3339, when known. */
  askedAt: string | null;
}

/** The answer sent for one waiting decision. */
export interface DecideRequest {
  id: string;
  answer: PendingAnswer;
  /** Why, for a refusal; handed to the agent. */
  reason?: string | null;
  /** The answers to the questions of an agent, one per question. */
  answers?: AskUserAnswer[];
  /** The answer to the typed question of a paused task. */
  value?: string | number | boolean;
}

/** What an answer did. */
export interface DecideResponse {
  id: string;
  answer: PendingAnswer;
  /** How many waiting runs, conversations or items went on. */
  resumed: number;
  /** How many other batch items the answer covers ran again. */
  batchItemsResumed: number;
  message: string;
}

/**
 * Lists every decision waiting for the user. An answer without its `items`
 * list rejects: published as is, the sidebar count read `.length` on
 * `undefined` inside a store subscriber, which leaves the module-wide queue of
 * `svelte/store` non-empty and silences every store of the window after it.
 */
export async function listPendingDecisions(): Promise<PendingDecision[]> {
  const listed = await invoke<{ items?: unknown } | null>("list_pending_decisions");
  if (!Array.isArray(listed?.items)) {
    throw new Error("list_pending_decisions answered without an items list");
  }
  return listed.items as PendingDecision[];
}

/** Answers one waiting decision, through the call that answers it where it was born. */
export async function decidePendingDecision(request: DecideRequest): Promise<DecideResponse> {
  return invoke<DecideResponse>("decide_pending_decision", { request });
}
