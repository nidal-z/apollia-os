/**
 * What a conversation must put back on screen when it (re)mounts.
 *
 * The backend keeps streaming while the operator is on another page, and the
 * global stores keep the tokens, the pending approval and the pending ask_user
 * request. A conversation that mounted without reading them showed an idle
 * thread over a turn that was still running.
 *
 * The decision is returned as a patch rather than applied here, so it stays a
 * pure function of the stores and the component keeps the single place that
 * writes its own state.
 */
import type { AskUserQuestion } from "$lib/types";

export interface ConversationRestoreInput {
  sessionId: string;
  /** Snapshot of `globalTokenBuffers`. */
  buffers: Record<string, string>;
  sessionStatus: "active" | "processing" | "closed";
  approval: {
    sessionId: string;
    messageId: string;
    toolCallId: string;
    toolName: string;
    inputPreview: string;
  } | null;
  userInput: { request_id: string; questions_json: string; context: string | null } | null;
  /** True when an ask_user card is already on screen; it is not replaced. */
  hasUserInput: boolean;
}

export interface ConversationRestorePatch {
  tokenBuffer?: string;
  isStreaming?: boolean;
  isProcessing?: boolean;
  approval?: ConversationRestoreInput["approval"];
  userInput?: {
    requestId: string;
    questions: AskUserQuestion[];
    context: string | null;
  };
  /** True when the caller should scroll to the newest content. */
  scroll: boolean;
}

export function restoreConversationState(
  input: ConversationRestoreInput,
): ConversationRestorePatch {
  const patch: ConversationRestorePatch = { scroll: false };
  const bufferedText = input.buffers[input.sessionId];
  if (bufferedText) {
    patch.tokenBuffer = bufferedText;
    patch.isStreaming = true;
    patch.isProcessing = false;
    patch.scroll = true;
  }

  if (input.approval) {
    patch.approval = input.approval;
    patch.isStreaming = false;
    patch.scroll = true;
  }

  if (input.userInput && !input.hasUserInput) {
    try {
      patch.userInput = {
        requestId: input.userInput.request_id,
        questions: JSON.parse(input.userInput.questions_json),
        context: input.userInput.context,
      };
      patch.isStreaming = false;
      patch.scroll = true;
    } catch {
      // Malformed questions_json - ignore, will be re-emitted by backend
    }
  }

  // Still processing with nothing to show yet: say so rather than look idle.
  if (
    input.sessionStatus === "processing" &&
    !bufferedText &&
    !input.approval &&
    !input.userInput
  ) {
    patch.isProcessing = true;
  }
  return patch;
}
