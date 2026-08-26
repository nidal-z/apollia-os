// ─── Speech-to-text ───

// ─── STT (Speech-to-Text) ────────────────────────────────────────────────────

/** Description of an available STT model file on disk. */
export interface SttModelInfo {
  name: string;
  path: string;
  size_mb: number;
  language: string | null;
}

/** STT configuration read from / written to the `[stt]` section of apollia.toml. */
export interface SttConfigView {
  enabled: boolean;
  model_path: string;
  hotkey: string;
  clipboard_mode: string;
  clipboard_restore: boolean;
  silence_threshold_db: number;
  max_recording_sec: number;
  language: string | null;
  trigger_mode: string;
  input_device: string | null;
}

/** STT configuration - CRUD view with strict types (mirrors `SttConfigRow`). */
export interface SttConfig {
  enabled: boolean;
  model_path: string;
  hotkey: string;
  clipboard_mode: "paste" | "clipboard";
  clipboard_restore: boolean;
  silence_threshold_db: number;
  max_recording_sec: number;
  language: string | null;
  trigger_mode: "toggle" | "push-to-talk";
  input_device: string | null;
}

/** Current status of the STT engine reported by `get_stt_status`. */
export interface SttStatus {
  enabled: boolean;
  model_loaded: boolean;
  model_path: string;
  model_name: string;
  backend_name: string;
  metal_enabled: boolean;
  cuda_enabled: boolean;
  /** Whether at least one audio input device (microphone) is present. */
  input_available?: boolean;
}

/** A single transcription row returned by `list_transcriptions`. */
export interface TranscriptRow {
  id: string;
  full_text: string;
  language: string | null;
  source: "hotkey" | "file" | "api";
  audio_duration_ms: number;
  processing_time_ms: number;
  model_name: string | null;
  created_at: string;
}
