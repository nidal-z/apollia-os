/**
 * Typed Tauri IPC wrappers for the chat surface.
 *
 * Every `invoke` the chat conversation, its config panel and the sessions
 * sidebar need lives here, so the `.svelte` files call typed helpers instead
 * of stringly-typed Tauri commands. Wrapper names are `camelCase` of the Rust
 * command name (`crates/apollia-desktop/ui/AGENTS.md` section 6).
 */
import { invoke } from "@tauri-apps/api/core";
import type { TurnLlmCall } from "$lib/chat/generationStats";
import type {
  ChatImageData,
  ChatModelCapabilities,
  ChatSessionDetail,
  ConversationStatsView,
  UpdateSessionRequest,
} from "$lib/types";

/** Full detail of one chat session (messages, config, linkage). */
export function getChatSession(sessionId: string): Promise<ChatSessionDetail> {
  return invoke<ChatSessionDetail>("get_chat_session", { sessionId });
}

/** Interrupt the in-flight generation of a session. */
export function pauseChatSession(sessionId: string): Promise<void> {
  return invoke<void>("pause_chat_session", { sessionId });
}

/**
 * Send a user message; resolves with the created message id. `images` travel
 * after the text, for a model that reads images.
 */
export function sendChatMessage(
  sessionId: string,
  content: string,
  images: ChatImageData[] = [],
): Promise<string> {
  return invoke<string>("send_chat_message", { sessionId, content, images });
}

/** What the model of a conversation can do: images and their formats. */
export function getChatModelCapabilities(sessionId: string): Promise<ChatModelCapabilities> {
  return invoke<ChatModelCapabilities>("get_chat_model_capabilities", { sessionId });
}

/** One stored image of a conversation, by the file name its message lists. */
export function readChatImage(sessionId: string, file: string): Promise<ChatImageData> {
  return invoke<ChatImageData>("read_chat_image", { sessionId, file });
}

/** Regenerate the assistant answer that follows `messageId`. */
export function regenerateChatResponse(
  sessionId: string,
  messageId: string,
): Promise<void> {
  return invoke<void>("regenerate_chat_response", { sessionId, messageId });
}

/** Replace a user message and re-run the exchange; resolves with the new id. */
export function editAndResendChatMessage(
  sessionId: string,
  messageId: string,
  content: string,
): Promise<string> {
  return invoke<string>("edit_and_resend_chat_message", {
    sessionId,
    messageId,
    content,
  });
}

/** Write an exported conversation to `destPath` with the given MIME type. */
export function exportConversation(
  destPath: string,
  content: string,
  mime: string,
): Promise<void> {
  return invoke<void>("export_conversation", { destPath, content, mime });
}

/** Rename a session. */
export function renameChatSession(sessionId: string, title: string): Promise<void> {
  return invoke<void>("rename_chat_session", { sessionId, title });
}

/** Delete a session and its messages. */
export function deleteChatSession(sessionId: string): Promise<void> {
  return invoke<void>("delete_chat_session", { sessionId });
}

/** Update the per-session config (system prompt, tools, backend). */
export function updateChatSession(
  sessionId: string,
  update: UpdateSessionRequest,
): Promise<void> {
  return invoke<void>("update_chat_session", { sessionId, update });
}

/** Token and cost counters of one conversation. */
export function getConversationStats(
  sessionId: string,
): Promise<ConversationStatsView> {
  return invoke<ConversationStatsView>("get_conversation_stats", { sessionId });
}

/**
 * The model calls of one turn, in order. The turn is named by the identifier
 * of its answer message.
 */
export function listTurnLlmCalls(sessionId: string, turnId: string): Promise<TurnLlmCall[]> {
  return invoke<TurnLlmCall[]>("list_turn_llm_calls", { sessionId, turnId });
}
