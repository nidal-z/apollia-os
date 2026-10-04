/**
 * Typed Tauri IPC wrapper behind the selector of a conversation: the agents
 * and the models the chat can talk to, read by `list_chat_targets`
 * (`crates/apollia-desktop/src/commands/chat/targets.rs`).
 */
import { invoke } from "@tauri-apps/api/core";
import type { ChatTargetsPayload } from "$lib/chat/chatTargets";

/** The agents in force, the installed models, the servers and their models. */
export function listChatTargets(): Promise<ChatTargetsPayload> {
  return invoke<ChatTargetsPayload>("list_chat_targets");
}
