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
  ChatDocumentData,
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
 * after the text, for a model that reads images. `documents` are read by the
 * runtime on reception: their text joins the message.
 */
export function sendChatMessage(
  sessionId: string,
  content: string,
  images: ChatImageData[] = [],
  documents: ChatDocumentData[] = [],
): Promise<string> {
  return invoke<string>("send_chat_message", { sessionId, content, images, documents });
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

/** Delete a session and its messages, for good: nothing goes to the trash. */
export function deleteChatSession(sessionId: string): Promise<void> {
  return invoke<void>("delete_chat_session", { sessionId });
}

/** A conversation in the trash, as `list_trashed_chat_sessions` returns it. */
export interface TrashedChatSession {
  id: string;
  mode: string;
  agent_name: string | null;
  title: string | null;
  created_at: string;
  deleted_at: string;
}

/** Send a session to the trash: it leaves the list and keeps its messages. */
export function trashChatSession(sessionId: string): Promise<void> {
  return invoke<void>("trash_chat_session", { sessionId });
}

/** Take a session out of the trash, back into the list. */
export function restoreChatSession(sessionId: string): Promise<void> {
  return invoke<void>("restore_chat_session", { sessionId });
}

/** The sessions in the trash, the last removed first. */
export function listTrashedChatSessions(): Promise<TrashedChatSession[]> {
  return invoke<TrashedChatSession[]>("list_trashed_chat_sessions");
}

/** A folder of the conversation list, as `list_chat_folders` returns it. */
export interface ChatFolder {
  id: string;
  name: string;
  position: number;
  /** The conversations in the folder, the ones in the trash left out. */
  session_ids: string[];
}

/** A conversation whose messages contain the words searched. */
export interface MessageSearchHit {
  session_id: string;
  title: string | null;
  agent_name: string | null;
  created_at: string;
  /** The passage of the best matching message, on one line. */
  excerpt: string;
}

/** The folders of the conversation list, in their order. */
export function listChatFolders(): Promise<ChatFolder[]> {
  return invoke<ChatFolder[]>("list_chat_folders");
}

/** Create a folder, placed after the existing ones. */
export function createChatFolder(name: string): Promise<ChatFolder> {
  return invoke<ChatFolder>("create_chat_folder", { name });
}

/** Give a folder a new name. */
export function renameChatFolder(folderId: string, name: string): Promise<void> {
  return invoke<void>("rename_chat_folder", { folderId, name });
}

/** Remove a folder; its conversations are kept, out of any folder. */
export function deleteChatFolder(folderId: string): Promise<void> {
  return invoke<void>("delete_chat_folder", { folderId });
}

/** Put a session in a folder, or out of any folder with `null`. */
export function moveChatSessionToFolder(
  sessionId: string,
  folderId: string | null,
): Promise<void> {
  return invoke<void>("move_chat_session_to_folder", { sessionId, folderId });
}

/** The conversations whose messages contain every word of `query`. */
export function searchChatMessages(query: string, limit?: number): Promise<MessageSearchHit[]> {
  return invoke<MessageSearchHit[]>("search_chat_messages", { query, limit });
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
