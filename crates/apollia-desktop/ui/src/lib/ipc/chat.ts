/**
 * Typed Tauri IPC wrappers for the chat surface.
 *
 * Every `invoke` the chat conversation, its config panel and the sessions
 * sidebar need lives here, so the `.svelte` files call typed helpers instead
 * of stringly-typed Tauri commands. Wrapper names are `camelCase` of the Rust
 * command name (`crates/apollia-desktop/ui/AGENTS.md` section 6).
 */
import { invoke } from "@tauri-apps/api/core";
import type {
  ChatSessionDetail,
  ChatSessionSummary,
  ConversationStatsView,
  UpdateSessionRequest,
} from "$lib/types";

/** Full detail of one chat session (messages, config, linkage). */
export function getChatSession(sessionId: string): Promise<ChatSessionDetail> {
  return invoke<ChatSessionDetail>("get_chat_session", { sessionId });
}

/** Every stored chat session, newest first. */
export function listChatSessions(): Promise<ChatSessionSummary[]> {
  return invoke<ChatSessionSummary[]>("list_chat_sessions");
}

/** Interrupt the in-flight generation of a session. */
export function pauseChatSession(sessionId: string): Promise<void> {
  return invoke<void>("pause_chat_session", { sessionId });
}

/** Send a user message; resolves with the created message id. */
export function sendChatMessage(sessionId: string, content: string): Promise<string> {
  return invoke<string>("send_chat_message", { sessionId, content });
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

/** Attach a session to a project, or detach it with `projectId: null`. */
export function linkChatToProject(
  sessionId: string,
  projectId: string | null,
): Promise<void> {
  return invoke<void>("link_chat_to_project", { sessionId, projectId });
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

/** Stored defaults applied to every free-chat session. */
export interface ChatLibreConfig {
  system_prompt: string;
  allowed_tools: string[];
  llm_backend: string | null;
}

/** Read the free-chat defaults. */
export function getChatLibreConfig(): Promise<ChatLibreConfig> {
  return invoke<ChatLibreConfig>("get_chat_libre_config");
}

/** Persist the free-chat defaults. */
export function updateChatLibreConfig(config: ChatLibreConfig): Promise<void> {
  return invoke<void>("update_chat_libre_config", { config });
}
