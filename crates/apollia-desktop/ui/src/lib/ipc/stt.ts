/**
 * Typed Tauri IPC wrappers for the Speech-to-Text domain.
 *
 * One thin function per `#[tauri::command]` in
 * `crates/apollia-desktop/src/commands/stt.rs`. Components and the STT store
 * call these instead of `invoke()` directly, so command names and payload
 * shapes live in a single place.
 */
import { invoke } from "@tauri-apps/api/core";
import type { SttConfigView, SttStatus, TranscriptRow } from "$lib/types";

/** Fetch the current STT engine status. */
export async function getSttStatus(): Promise<SttStatus> {
  return invoke<SttStatus>("get_stt_status");
}

/** Persist the STT configuration; the runtime hot-reloads the engine. */
export async function updateSttConfig(config: SttConfigView): Promise<void> {
  return invoke<void>("update_stt_config", { config });
}

/** Rebuild the STT engine from the database and re-arm the global hotkey. */
export async function reloadStt(): Promise<void> {
  return invoke<void>("reload_stt");
}

/** List the available audio input devices (microphones). */
export async function listAudioInputDevices(): Promise<string[]> {
  return invoke<string[]>("list_audio_input_devices");
}

/**
 * Copy a whisper weights file into `~/.apollia/models/` and return the
 * destination path. `allowedExtensions` guards the import against non-model
 * files.
 */
export async function importModelFile(
  sourcePath: string,
  allowedExtensions: string[],
): Promise<string> {
  return invoke<string>("import_model_file", { sourcePath, allowedExtensions });
}

/** Start the throwaway test recorder used by the dictation self-test. */
export async function startTourRecording(): Promise<void> {
  return invoke<void>("start_tour_recording");
}

/** Stop the test recorder; transcription arrives via the `stt-transcribed` event. */
export async function stopTourRecording(): Promise<void> {
  return invoke<void>("stop_tour_recording");
}

/** List the most recent transcriptions, newest first. */
export async function listTranscriptions(limit = 50): Promise<TranscriptRow[]> {
  return invoke<TranscriptRow[]>("list_transcriptions", { limit });
}

/** Delete a transcription by id. */
export async function deleteTranscription(id: string): Promise<void> {
  return invoke<void>("delete_transcription", { id });
}

/** Transcribe an audio file at `filePath` and return the created row. */
export async function transcribeFile(filePath: string): Promise<TranscriptRow> {
  return invoke<TranscriptRow>("transcribe_file", { filePath });
}
