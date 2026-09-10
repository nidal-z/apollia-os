/**
 * What the "active model" row of the dictation settings says about the engine.
 *
 * `model_loaded` in the runtime's status means one thing: a transcription has
 * come back on this engine instance. The sidecar loads the model on its first
 * request, so that reading is the only proof the daemon has that the file on
 * disk is one the engine can load. Every reload (a new microphone, hotkey or
 * model) rebuilds the instance and the flag starts false again.
 *
 * The row used to render that flag as "model not loaded", which an operator
 * reads as a failure, right after a dictation that worked and landed in the
 * history (reported on 2026-09-09 on Windows). Three states, named for what
 * the operator can act on:
 *
 * - `absent`: no model configured, or the engine is disabled;
 * - `ready`: a model is configured and the engine armed, nothing transcribed
 *   yet, so it loads on the first request;
 * - `loaded`: a transcription came back, the model is resident.
 */
export type ModelRowState = "absent" | "ready" | "loaded";

export interface ModelRowInput {
  enabled: boolean;
  model_loaded: boolean;
  model_name: string;
}

export function modelRowState(status: ModelRowInput | null): ModelRowState {
  if (status === null || !status.enabled || status.model_name.trim() === "") {
    return "absent";
  }
  return status.model_loaded ? "loaded" : "ready";
}
