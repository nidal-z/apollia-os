/**
 * Svelte stores for the Speech-to-Text feature.
 *
 * Provides reactive state for STT engine status, transcription history, and
 * recording state.  Hydrated via Tauri IPC commands and kept in sync through
 * Tauri event listeners (`stt-transcribed`, `stt-recording-started`,
 * `stt-recording-stopped`).
 */
import { writable } from "svelte/store";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { SttStatus, TranscriptRow } from "$lib/types";

/** Current status of the STT engine (`null` until first fetch). */
export const sttStatus = writable<SttStatus | null>(null);

/** List of transcription rows, most recent first. */
export const transcriptions = writable<TranscriptRow[]>([]);

/** Whether the microphone is currently recording. */
export const isRecording = writable<boolean>(false);

// ─── IPC helpers ─────────────────────────────────────────────────────────────

/** Fetch STT engine status from the runtime. */
export async function refreshSttStatus(): Promise<void> {
  try {
    const result: SttStatus = await invoke("get_stt_status");
    sttStatus.set(result);
  } catch {
    // runtime or STT engine not ready yet — keep current state
  }
}

/** Fetch transcription list from the runtime. */
export async function refreshTranscriptions(): Promise<void> {
  try {
    const result: TranscriptRow[] = await invoke("list_transcriptions", {
      limit: 50,
    });
    transcriptions.set(result);
  } catch {
    // runtime not ready yet — keep current state
  }
}

/** Delete a transcription by ID and refresh the list. */
export async function deleteTranscription(id: string): Promise<void> {
  await invoke("delete_transcription", { id });
  await refreshTranscriptions();
}

/** Transcribe an audio file and refresh the list. */
export async function transcribeFile(filePath: string): Promise<TranscriptRow> {
  const result: TranscriptRow = await invoke("transcribe_file", {
    filePath,
  });
  await refreshTranscriptions();
  return result;
}

// ─── Event listeners ─────────────────────────────────────────────────────────

/**
 * Subscribe to STT-related Tauri events.  Returns a cleanup function that
 * unregisters all listeners.  Should be called once from the root layout or
 * from the SSE connection setup.
 */
export function listenSttEvents(): () => void {
  const unlisteners: Promise<UnlistenFn>[] = [];

  unlisteners.push(
    listen("stt-transcribed", () => {
      void refreshTranscriptions();
      void refreshSttStatus();
    }),
  );

  unlisteners.push(
    listen("stt-recording-started", () => {
      isRecording.set(true);
    }),
  );

  unlisteners.push(
    listen("stt-recording-stopped", () => {
      isRecording.set(false);
    }),
  );

  return () => {
    for (const p of unlisteners) {
      void p.then((fn) => fn());
    }
  };
}
