/**
 * Typed Tauri IPC wrappers behind the Settings tab of a conversation.
 *
 * Thin wrappers over the commands of
 * `crates/apollia-desktop/src/commands/chat/settings.rs`, each of which calls
 * one runtime route: the settings of a conversation read and replaced, its
 * notes read and replaced, its prompt template read, a preset created. The runtime validates what it
 * stores and a refusal rejects with its text.
 */
import { invoke } from "@tauri-apps/api/core";
import type { PresetDraft, SessionPreset, SessionSettings } from "$lib/chat/sessionSettings";

/** The settings of the conversation; a field left out is inherited. */
export function getChatSessionSettings(sessionId: string): Promise<SessionSettings> {
  return invoke<SessionSettings>("get_chat_session_settings", { sessionId });
}

/** Replaces the settings of the conversation; answers them as stored. */
export function setChatSessionSettings(
  sessionId: string,
  settings: SessionSettings,
): Promise<SessionSettings> {
  return invoke<SessionSettings>("set_chat_session_settings", { sessionId, settings });
}

/** The notes of the conversation. */
export function getChatSessionNotes(sessionId: string): Promise<string> {
  return invoke<string>("get_chat_session_notes", { sessionId });
}

/** Replaces the notes of the conversation; answers them as stored. */
export function setChatSessionNotes(sessionId: string, notes: string): Promise<string> {
  return invoke<string>("set_chat_session_notes", { sessionId, notes });
}

/**
 * The prompt template the model of the conversation receives and where it
 * comes from: its preset, the engine serving the model, or nowhere readable.
 */
export function getChatPromptTemplate(sessionId: string): Promise<unknown> {
  return invoke<unknown>("get_chat_prompt_template", { sessionId });
}

/** Creates a preset of a model; answers it with its identifier. */
export function createModelPreset(preset: PresetDraft): Promise<SessionPreset> {
  return invoke<SessionPreset>("create_model_preset", { preset });
}
