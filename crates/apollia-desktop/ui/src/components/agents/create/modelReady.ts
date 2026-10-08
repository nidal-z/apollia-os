/**
 * Whether the model the creation conversation will speak to can answer.
 *
 * The conversation goes to the default model. When that model is served by a
 * declared server, the server is asked for its models first: one that does
 * not answer is named before a message is sent, where the conversation used
 * to start and fail on its first turn.
 */
import { getDefaultModel, listLlmServerModels, type DefaultModel } from "$lib/ipc/llmServers";

export type ModelReadiness = { ready: true } | { ready: false; server: string };

interface Probes {
  defaultModel: () => Promise<DefaultModel | null>;
  serverModels: (name: string) => Promise<unknown>;
}

const PROBES: Probes = { defaultModel: getDefaultModel, serverModels: listLlmServerModels };

/**
 * A default model on this machine, or none, is left to the conversation,
 * which says what it lacks. Only a server that does not answer stops here. A
 * default that cannot be read does not stop the conversation either.
 */
export async function modelReadiness(probes: Probes = PROBES): Promise<ModelReadiness> {
  let chosen: DefaultModel | null;
  try {
    chosen = await probes.defaultModel();
  } catch {
    return { ready: true };
  }
  if (chosen === null || chosen.model_source !== "server") return { ready: true };
  try {
    await probes.serverModels(chosen.backend);
    return { ready: true };
  } catch {
    return { ready: false, server: chosen.backend };
  }
}
