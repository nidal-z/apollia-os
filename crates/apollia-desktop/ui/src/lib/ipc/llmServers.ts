/**
 * Typed Tauri IPC wrappers for the inference servers: testing, saving,
 * listing, changing and forgetting one, and the default model
 * (`crates/apollia-desktop/src/commands/llm_servers.rs`).
 */
import { invoke } from "@tauri-apps/api/core";

/** What a refused server test is rejected with. */
export interface ServerTestRefusal {
  /** `unreachable`, `key_refused`, `not_openai_compatible`, `policy` or `other`. */
  kind: string;
  /** The sentence of the runtime. */
  message: string;
}

/** A server as the runtime saved it. The key is never part of it. */
export interface DeclaredServer {
  name: string;
  base_url: string;
  has_key: boolean;
  api_key_env: string | null;
  enabled: boolean;
  /** Seconds the server may stay silent; `null` keeps the default. */
  timeout_secs: number | null;
  /** Fields added to every request sent to the server; `null` when none. */
  extra_params: Record<string, unknown> | null;
}

/** One model a saved server lists. */
export interface ServerModel {
  /** Identifier the server gives the model. */
  id: string;
  /** Name the model is addressed by, `<server>/<model>`. */
  route: string;
  /** Window the server serves the model with, when it says so. */
  context_window: number | null;
  /** Whether that window is shorter than the one an agent is expected to have. */
  context_short: boolean;
}

/** The models of a saved server, and the window an agent is expected to have. */
export interface ServerModels {
  server: string;
  models: ServerModel[];
  expected_context_window: number;
}

/**
 * What to change on a saved server. A field left out keeps what the server
 * has; `null` empties the key, the timeout or the extra parameters.
 */
export interface ServerChanges {
  base_url?: string;
  api_key?: string | null;
  enabled?: boolean;
  timeout_secs?: number | null;
  extra_params?: Record<string, unknown> | null;
}

/** The default model of the machine. */
export interface DefaultModel {
  /** Name the router routes it by. */
  backend: string;
  model_source: "local" | "server";
  model_id: string;
}

/**
 * Lists the models of the server at `baseUrl`, saving nothing. Rejects with a
 * {@link ServerTestRefusal}.
 */
export function testLlmServer(baseUrl: string, apiKey: string | null): Promise<string[]> {
  return invoke<string[]>("test_llm_server", { baseUrl, apiKey });
}

/** Saves the server `name`, replacing one of the same name. */
export function saveLlmServer(
  name: string,
  baseUrl: string,
  apiKey: string | null,
): Promise<DeclaredServer> {
  return invoke<DeclaredServer>("save_llm_server", { name, baseUrl, apiKey });
}

/** Makes `modelId` the default model; a server model is `<server>/<model>`. */
export function setDefaultModel(
  modelSource: "local" | "server",
  modelId: string,
): Promise<DefaultModel> {
  return invoke<DefaultModel>("set_default_model", { modelSource, modelId });
}

/** The default model of the machine, `null` when none is set. */
export function getDefaultModel(): Promise<DefaultModel | null> {
  return invoke<DefaultModel | null>("get_default_model");
}

/** Lists the saved servers. */
export function listLlmServers(): Promise<DeclaredServer[]> {
  return invoke<DeclaredServer[]>("list_llm_servers");
}

/**
 * Lists the models of the saved server `name`, reached with the key it was
 * saved with. Rejects with a {@link ServerTestRefusal}.
 */
export function listLlmServerModels(name: string): Promise<ServerModels> {
  return invoke<ServerModels>("list_llm_server_models", { name });
}

/** Changes the saved server `name`; only the fields of `changes` move. */
export function updateLlmServer(name: string, changes: ServerChanges): Promise<DeclaredServer> {
  return invoke<DeclaredServer>("update_llm_server", { name, changes });
}

/** Forgets the saved server `name` and the key it had. */
export function deleteLlmServer(name: string): Promise<void> {
  return invoke<void>("delete_llm_server", { name });
}
