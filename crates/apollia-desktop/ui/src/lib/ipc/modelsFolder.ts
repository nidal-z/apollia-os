// Typed IPC wrappers for the models block of the General settings: the models
// folder in force and the Hugging Face token kept in the secret store.

/** The models folder in force: `[models] dir` of `apollia.toml`, else the default. */
export async function getModelsDir(): Promise<string> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<string>("get_models_dir");
}

/** The stored Hugging Face token, or `null` when there is none. */
export async function getHfToken(): Promise<string | null> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<string | null>("get_hf_token");
}

/** Stores the Hugging Face token in the secret store, replacing the previous one. */
export async function setHfToken(token: string): Promise<void> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<void>("set_hf_token", { token });
}

/** Removes the stored Hugging Face token. */
export async function clearHfToken(): Promise<void> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<void>("clear_hf_token");
}
