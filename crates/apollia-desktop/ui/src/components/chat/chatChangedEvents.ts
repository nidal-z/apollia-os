/**
 * The router of the `chat-changed` runtime events, for one conversation.
 *
 * Seven event types reach it, each carrying its payload in one of two shapes:
 * externally-tagged serde (`{ ChatApprovalRequired: { ... } }`) or flat. Both
 * are accepted, because both have been observed on the wire.
 *
 * Everything the router touches arrives through `ChatChangedPort`, so the rules
 * stay readable next to each other and the conversation component keeps only the
 * state they act on. Two of them are subtle enough to be worth stating here: an
 * approval pauses the turn without ending the stream, so the reasoning already
 * on screen is the context the operator judges the approval on; and a resolution
 * that names another tool call must not dismiss the card currently shown.
 */
/** The shape the `runtime-event` listener receives. */
export interface ChatChangedEvent {
  category: string;
  event_type: string;
  payload: Record<string, unknown>;
}

/** Everything the router reads or writes in the conversation. */
export interface ChatChangedPort {
  readonly sessionId: string;
  /** Reasoning fragments closed so far, used to order a tool row after them. */
  closedReasoningCount(): number;
  addToolCall(name: string, reasoningCursor: number): void;
  completeLastToolCall(success: boolean): void;
  readonly pendingApprovalToolCallId: string | null;
  setApproval(
    approval: {
      sessionId: string;
      messageId: string;
      toolCallId: string;
      toolName: string;
      inputPreview: string;
    } | null,
  ): void;
  forgetApproval(resolvedId: string | undefined): void;
  setUserInput(
    input: {
      requestId: string;
      questions: unknown[];
      context: string | null;
    } | null,
  ): void;
  forgetUserInput(requestId: string): void;
  setStreaming(streaming: boolean): void;
  setProcessing(processing: boolean): void;
  setPendingError(detail: string): void;
  /** Add (or replace) the system bubble that reports a failed exchange. */
  showErrorMessage(label: string): void;
  translate(key: string, values?: Record<string, string>): string;
  toast(label: string): void;
  scrollToBottom(): void;
  finalizeStreaming(): void;
  refreshSession(): void;
}

/** Read a payload that may be externally tagged under its own event name. */
function unwrap<T>(evt: ChatChangedEvent, tag: string): T {
  const inner = evt.payload?.[tag] as T | undefined;
  return inner ?? (evt.payload as T);
}

export function handleChatChangedEvent(
  evt: ChatChangedEvent,
  port: ChatChangedPort,
): void {
  if (evt.category !== "chat-changed") return;

  if (evt.event_type === "ChatToolCallStarted") {
    const p = evt.payload as { session_id?: string; tool_name?: string };
    if (p.session_id === port.sessionId) {
      port.addToolCall(p.tool_name ?? "?", port.closedReasoningCount());
      port.setApproval(null);
      port.scrollToBottom();
    }
    return;
  }
  if (evt.event_type === "ChatToolCallCompleted") {
    const p = evt.payload as { session_id?: string; success?: boolean };
    if (p.session_id === port.sessionId) {
      port.completeLastToolCall(p.success !== false);
    }
    return;
  }
  if (evt.event_type === "ChatApprovalRequired") {
    const p = unwrap<{
      session_id?: string;
      message_id?: string;
      tool_call_id?: string;
      tool_name?: string;
      prompt?: string;
    }>(evt, "ChatApprovalRequired");
    if (!p.session_id || p.session_id === port.sessionId) {
      port.setApproval({
        sessionId: port.sessionId,
        messageId: p.message_id ?? "",
        toolCallId: p.tool_call_id ?? p.tool_name ?? "",
        toolName: p.tool_name ?? "",
        inputPreview: p.prompt ?? "",
      });
      port.scrollToBottom();
    }
    return;
  }
  if (
    evt.event_type === "ChatApprovalResolved" ||
    evt.event_type === "ChatApprovalTimeout"
  ) {
    const p = unwrap<{ tool_call_id?: string }>(evt, evt.event_type);
    const resolvedId = p.tool_call_id;
    const shown = port.pendingApprovalToolCallId;
    if (!resolvedId || shown === null || shown === resolvedId) {
      port.setApproval(null);
    }
    port.forgetApproval(resolvedId);
    return;
  }
  if (evt.event_type === "ChatUserInputRequired") {
    const p = unwrap<{
      request_id?: string;
      session_id?: string;
      questions_json?: string;
      context?: string;
    }>(evt, "ChatUserInputRequired");
    if (!p.session_id || p.session_id === port.sessionId || p.session_id === "") {
      try {
        const questions = JSON.parse(p.questions_json ?? "[]");
        port.setUserInput({
          requestId: p.request_id ?? "",
          questions,
          context: p.context ?? null,
        });
      } catch {
        console.warn("Failed to parse ask_user questions:", p.questions_json);
      }
      port.scrollToBottom();
    }
    return;
  }
  if (evt.event_type === "ChatUserInputResolved") {
    const p = unwrap<{ request_id?: string }>(evt, "ChatUserInputResolved");
    port.setUserInput(null);
    if (p.request_id) port.forgetUserInput(String(p.request_id));
    return;
  }
  if (evt.event_type === "ChatError") {
    const p = unwrap<{ session_id?: string; error?: string }>(evt, "ChatError");
    if (!p.session_id || p.session_id === port.sessionId || p.session_id === "") {
      port.setStreaming(false);
      port.setProcessing(false);
      const detail = p.error?.trim() ? p.error : port.translate("chat.exchange_error_generic");
      port.setPendingError(detail);
      const label = port.translate("chat.exchange_error", { error: detail });
      port.toast(label);
      port.showErrorMessage(label);
      port.scrollToBottom();
    }
    return;
  }
  if (evt.event_type === "ChatResponseCompleted") {
    port.setApproval(null);
    port.finalizeStreaming();
    return;
  }
  port.refreshSession();
}
