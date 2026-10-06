/**
 * Typed Tauri IPC wrappers for declaring an inference server
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
