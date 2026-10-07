// What the local server serves, as the developer screen lists it: the models
// in memory, and the agents and workflows marked exposed, which a client calls
// like models.
//
// The decisions live here, apart from the components, so Vitest can hold them
// without rendering anything.

import type { McpExposableItem, McpExposableKind } from "$lib/ipc/mcpServer";
import type { WorkflowView } from "$lib/ipc/workflows";
import type { ModelLoadAsk } from "$lib/stores/modelResidency";
import type { LoadedModel, LoadedModelsResponse } from "$lib/types";
import { ejectModel, type EjectAttempt } from "$lib/chat/modelLoad";
import { loadedFormat } from "../ai/aiModels";
import { unloadErrorKey } from "../ai/loadedModels";

/** The prefix of the identifier of an agent or a workflow served as a model. */
export const SERVED_PREFIX = "apollia/";

/** The kind of a model as the list of models of the server names it. */
export type ServedModelKind = "llm" | "embeddings";

/** The models in memory, in the order the runtime lists them. */
export function servedModels(loaded: LoadedModelsResponse | null): LoadedModel[] {
  return loaded?.models ?? [];
}

/** The kind the server gives a model in memory. */
export function servedModelKind(model: LoadedModel): ServedModelKind {
  return model.kind === "embeddings" ? "embeddings" : "llm";
}

/**
 * The identifier a client names the model by: the one of the model index,
 * else the name of its file.
 */
export function servedModelId(model: LoadedModel): string {
  return model.modelId ?? model.model;
}

/** The size of the weights, in bytes; `null` when the index does not hold the model. */
export function servedModelSize(model: LoadedModel): number | null {
  return typeof model.sizeBytes === "number" ? model.sizeBytes : null;
}

/** The requests the model answers at once; `null` when the engine default applies. */
export function servedModelParallel(model: LoadedModel): number | null {
  return typeof model.nParallel === "number" ? model.nParallel : null;
}

/**
 * What the loading window opens on for the detail and the settings of a
 * model in memory; `null` for a file the model index does not hold, which
 * the window cannot name.
 */
export function detailAsk(model: LoadedModel, onloaded: () => void): ModelLoadAsk | null {
  if (!model.modelId) return null;
  return { modelId: model.modelId, format: loadedFormat(model), onloaded };
}

/** A text for the reader, as a catalogue key and its values. */
export interface Notice {
  key: string;
  values: Record<string, string>;
  tone: "success" | "error";
}

/** What the screen says once an eject answered; nothing when nothing was asked. */
export function ejectNotice(attempt: EjectAttempt): Notice | null {
  if (attempt.kind === "nothing") return null;
  const name = servedModelId(attempt.model);
  if (attempt.kind === "ejected") {
    return { key: "ai.loaded.unloaded_toast", values: { name }, tone: "success" };
  }
  const refusal = unloadErrorKey(attempt.message);
  if (refusal) return { key: refusal, values: { name }, tone: "error" };
  return {
    key: "dev.developer.models.eject_failed",
    values: { name, error: attempt.message },
    tone: "error",
  };
}

/**
 * Ejects `model` through `unload`, then reads the resident models again
 * whatever the answer, so the cards show what the runtime holds.
 */
export async function ejectServed(
  unload: (modelPath: string) => Promise<unknown>,
  refresh: () => Promise<unknown>,
  model: LoadedModel,
): Promise<Notice | null> {
  const attempt = await ejectModel(unload, model);
  await refresh();
  return ejectNotice(attempt);
}

/** What a call to an exposed item takes. */
export type ServedInputs =
  /** An agent takes a message. */
  | { kind: "message" }
  /** A workflow takes an object with these fields, in the declared order. */
  | { kind: "fields"; names: string[] }
  /** The inputs of the workflow could not be read. */
  | { kind: "unknown" };

/** An agent or a workflow the server serves. */
export interface ServedItem {
  kind: McpExposableKind;
  /** The agent id, or `<package>/<workflow>`. */
  itemId: string;
  /** `apollia/<id>`, what a client puts in `model`. */
  identifier: string;
  name: string;
  inputs: ServedInputs;
}

/** The identifier a client calls the agent or workflow `itemId` by. */
export function servedIdentifier(itemId: string): string {
  return `${SERVED_PREFIX}${itemId}`;
}

/** The shell calls the list of exposed items makes, replaceable in tests. */
export interface ServedItemsIpc {
  /** Every agent and workflow, each with its flag. */
  list: () => Promise<McpExposableItem[]>;
  /** One workflow, with the fields of its inputs. */
  workflow: (workflowId: string) => Promise<WorkflowView>;
}

async function inputsOf(ipc: ServedItemsIpc, item: McpExposableItem): Promise<ServedInputs> {
  if (item.kind !== "workflow") return { kind: "message" };
  try {
    const view = await ipc.workflow(item.item_id);
    return { kind: "fields", names: view.inputs.map((field) => field.name) };
  } catch {
    return { kind: "unknown" };
  }
}

/**
 * The agents and workflows marked exposed, in the order of the list, each
 * with what a call to it takes. An item that is not exposed is left out: the
 * server does not serve it.
 */
export async function readServedItems(ipc: ServedItemsIpc): Promise<ServedItem[]> {
  const exposed = (await ipc.list()).filter((item) => item.exposed);
  return Promise.all(
    exposed.map(async (item) => ({
      kind: item.kind,
      itemId: item.item_id,
      identifier: servedIdentifier(item.item_id),
      name: item.name,
      inputs: await inputsOf(ipc, item),
    })),
  );
}

/** The id of an exposed item as a test id carries it: no slash, stable across names. */
export function servedItemSlug(item: ServedItem): string {
  return item.itemId.replace(/[^A-Za-z0-9_-]/g, "_");
}

/** Copies `text`; `false` when the copy fails. */
export async function copyIdentifier(
  text: string,
  write: (text: string) => Promise<void>,
): Promise<boolean> {
  try {
    await write(text);
    return true;
  } catch {
    return false;
  }
}

/**
 * The runtime events after which the models in memory, or what they are
 * doing, may have changed: a load, an unload, a call that starts or ends.
 */
const RESIDENCY_EVENTS: ReadonlySet<string> = new Set([
  "LlmModelLoading",
  "LlmModelReady",
  "LlmModelReloaded",
  "LlmModelUnloaded",
  "LlmModelFailed",
  "LlmCallStarted",
  "LlmCallCompleted",
  "LlmCallFailed",
]);

/** Whether a runtime event of type `eventType` may have moved the models in memory. */
export function movesResidency(eventType: string): boolean {
  return RESIDENCY_EVENTS.has(eventType);
}

/** What a runtime event carries that this screen reads. */
export interface ResidencyEvent {
  event_type: string;
}

/** Subscribes to the runtime events; answers what stops the subscription. */
export type RuntimeListen = (
  handler: (event: ResidencyEvent) => void,
) => Promise<() => void> | (() => void);

/**
 * Reads the models in memory again at every runtime event that may have
 * moved them, so the cards follow loads and unloads made anywhere: this
 * screen, the chat, a client of the server, the idle sweep. Answers what
 * stops following.
 */
export async function followResidency(
  listen: RuntimeListen,
  refresh: () => unknown,
): Promise<() => void> {
  return listen((event) => {
    if (movesResidency(event.event_type)) refresh();
  });
}

/** The page of the documentation that lists the endpoints the server answers. */
export const ENDPOINTS_PAGE = "reference/compatible-endpoints";

/** Where "Supported endpoints" leads. */
export type EndpointsDestination =
  /** The page, in the documentation pane of this screen. */
  | { kind: "pane"; page: string }
  /** The same page on the documentation site, in the browser. */
  | { kind: "site"; url: string };

/**
 * Where "Supported endpoints" leads: the documentation pane when the screen
 * can open a page in it, the documentation site otherwise.
 */
export function endpointsDestination(
  paneOpens: boolean,
  siteUrl: (path: string) => string,
): EndpointsDestination {
  if (paneOpens) return { kind: "pane", page: ENDPOINTS_PAGE };
  return { kind: "site", url: siteUrl(`/${ENDPOINTS_PAGE}`) };
}

/** Opens "Supported endpoints" where `endpointsDestination` says. */
export function openEndpoints(
  openPage: ((page: string) => void) | undefined,
  openSite: (url: string) => unknown,
  siteUrl: (path: string) => string,
): EndpointsDestination {
  const destination = endpointsDestination(openPage !== undefined, siteUrl);
  if (destination.kind === "pane") openPage?.(destination.page);
  else openSite(destination.url);
  return destination;
}
