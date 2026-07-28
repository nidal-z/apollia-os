/**
 * Resolution effects for an inbox item, routed to the right typed IPC wrapper.
 *
 * Split out of the route so the orchestrator keeps only presentation state.
 * Each function rejects with the runtime error string on failure, letting the
 * caller humanize it (`reportError`) rather than swallowing it.
 */
import type { AlwaysScope } from "$lib/components/operator/HITLCard.svelte";
import { approvePlan, rejectPlan } from "$lib/ipc/planMode";
import {
  authorizeChatTool,
  respondUserInputRejected,
  resumeTask,
} from "$lib/ipc/inbox";
import type { PendingChatApproval } from "$lib/types";
import type { InboxItem } from "./types";

/** Approve or reject an item, dispatching on its kind. */
export async function resolveItem(
  item: InboxItem,
  approved: boolean,
  reason?: string,
): Promise<void> {
  if (item.kind === "task") {
    await resumeTask(item.source.task_id, approved, reason ?? null);
    return;
  }
  if (item.kind === "ask_user") {
    // Structured answers flow through `respondUserInput`; the outer reject path
    // only forwards a refusal reason back to the agent.
    if (!approved && reason) {
      await respondUserInputRejected(item.source.request_id, reason);
    }
    return;
  }
  if (item.kind === "plan") {
    if (approved) await approvePlan(item.source.sessionId);
    else await rejectPlan(item.source.sessionId, reason ?? undefined);
    return;
  }
  const src = item.source as PendingChatApproval;
  await authorizeChatTool({
    sessionId: src.sessionId,
    messageId: src.messageId,
    toolCallId: src.toolCallId,
    toolName: src.toolName,
    decision: approved ? "accept" : "refuse",
    reason: reason ?? null,
  });
}

/** Grant a standing "always allow" for a chat tool at the chosen scope. */
export async function alwaysAccept(item: InboxItem, scope: AlwaysScope): Promise<void> {
  if (item.kind === "task" || item.kind === "ask_user" || item.kind === "plan") return;
  const src = item.source as PendingChatApproval;
  await authorizeChatTool({
    sessionId: src.sessionId,
    messageId: src.messageId,
    toolCallId: src.toolCallId,
    toolName: src.toolName,
    decision: "always_accept",
    scope,
  });
}
