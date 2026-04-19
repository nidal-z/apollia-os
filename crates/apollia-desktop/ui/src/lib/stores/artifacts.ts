/**
 * Session artifacts store (US-SP42-031).
 *
 * Mirrors the backend `chat_artifacts` table for a single active session.
 * The detection helper in `$lib/chat/artifactDetect.ts` is responsible for
 * calling `save_artifact` when tool outputs look artifact-worthy; this
 * store just caches the latest result list per session.
 */
import { invoke } from "@tauri-apps/api/core";
import { derived, get, writable } from "svelte/store";

/** Wire-format Artifact — matches `commands::artifacts::Artifact` in Rust. */
export interface Artifact {
  id: string;
  session_id: string;
  source_message_id: string | null;
  kind: string;
  language: string | null;
  source_tool: string | null;
  title: string;
  content: string;
  created_at: string;
  updated_at: string;
}

export interface SaveArtifactRequest {
  session_id: string;
  source_message_id: string | null;
  kind: string;
  language: string | null;
  source_tool: string | null;
  title: string;
  content: string;
}

/** Artifacts for the currently viewed session (most recent first). */
export const artifacts = writable<Artifact[]>([]);

/** Selected artifact id for the viewer. Null = list mode. */
export const selectedArtifactId = writable<string | null>(null);

/** Derived: the currently selected artifact object. */
export const selectedArtifact = derived(
  [artifacts, selectedArtifactId],
  ([$arts, $id]) => $arts.find((a) => a.id === $id) ?? null,
);

export async function loadArtifacts(sessionId: string): Promise<void> {
  const list = await invoke<Artifact[]>("list_artifacts", {
    sessionId,
  });
  artifacts.set(list);
}

export async function saveArtifact(
  request: SaveArtifactRequest,
): Promise<Artifact> {
  const saved = await invoke<Artifact>("save_artifact", { request });
  artifacts.update((list) => {
    if (list.some((a) => a.id === saved.id)) return list;
    return [saved, ...list];
  });
  return saved;
}

export async function updateArtifact(
  id: string,
  patch: { title?: string; content?: string },
): Promise<Artifact> {
  const updated = await invoke<Artifact>("update_artifact", {
    id,
    title: patch.title ?? null,
    content: patch.content ?? null,
  });
  artifacts.update((list) => list.map((a) => (a.id === id ? updated : a)));
  return updated;
}

export async function deleteArtifact(id: string): Promise<void> {
  await invoke("delete_artifact", { id });
  artifacts.update((list) => list.filter((a) => a.id !== id));
  if (get(selectedArtifactId) === id) {
    selectedArtifactId.set(null);
  }
}

export function clearArtifacts(): void {
  artifacts.set([]);
  selectedArtifactId.set(null);
}

/**
 * Pending text to append to the chat input. ChatInput subscribes to this
 * and clears the store after consuming. Carries a monotonic counter so a
 * repeated value still triggers an update.
 */
export const chatInputAppend = writable<{ seq: number; text: string } | null>(
  null,
);

let appendSeq = 0;
export function requestChatInputAppend(text: string): void {
  appendSeq += 1;
  chatInputAppend.set({ seq: appendSeq, text });
}
