/**
 * The decisions behind "Try with the example": which example comes with a
 * demonstration dataset, what the gesture does first, what follows an
 * installation it asked for, and how the copied folder becomes a source of
 * the agent.
 *
 * Kept free of Svelte and of the Tauri bridge so each decision is tested on
 * its own; the calls are handed in by the dialog.
 */
import type { Connection } from "$lib/types";
import type { ResolvedAgent } from "$lib/ipc/agentSheet";
import type { NewSource, Source } from "$lib/ipc/sources";
import type { ExampleCard } from "./agentExamples";

/** The examples the application carries a demonstration dataset for. */
export const EXAMPLES_WITH_DATASET: readonly string[] = ["qualify-requests"];

/** Whether `example` can be tried on a dataset the application carries. */
export function hasDataset(example: string): boolean {
  return EXAMPLES_WITH_DATASET.includes(example);
}

/** A trial under way: the example, and the agent the batch will run. */
export interface Trial {
  example: string;
  agent: string;
}

/** What "Try with the example" does first on a card. */
export type TrialStart =
  /** Not installed yet: the preview every package gets comes first. */
  | { kind: "install"; example: string }
  /** Installed: the copy dialog opens at once. */
  | { kind: "copy"; trial: Trial }
  /** No dataset, or no agent to run it: nothing to try. */
  | { kind: "none" };

/** What the gesture does on `card`. Nothing is copied by any of them. */
export function trialStart(card: ExampleCard): TrialStart {
  if (!hasDataset(card.name) || card.sheetAgent === null) return { kind: "none" };
  if (!card.installed) return { kind: "install", example: card.name };
  return { kind: "copy", trial: { example: card.name, agent: card.sheetAgent } };
}

/** What an installation announces: the package and the agents it brought. */
export interface InstalledPackage {
  name: string;
  agents: { id: string; name: string }[];
}

/**
 * The trial that follows an installation: the one `awaited` was waiting for,
 * on the first agent the package brought. `null` when another package was
 * installed, when nothing was awaited, or when it brought no agent.
 */
export function trialAfterInstall(awaited: string | null, installed: InstalledPackage | null): Trial | null {
  if (awaited === null || installed === null || installed.name !== awaited) return null;
  const agent = installed.agents[0]?.id;
  return agent === undefined ? null : { example: awaited, agent };
}

/** The dialog before the copy, or after it. */
export type TrialStage = "choose" | "copied";

/** Why "Copy the example" is refused; `null` when it may run. */
export function copyRefusal(destination: string, copying: boolean): "destination_missing" | "copying" | null {
  if (copying) return "copying";
  return destination.trim().length === 0 ? "destination_missing" : null;
}

/** The calls that make a folder a source of an agent. */
export interface SourceSteps {
  listConnections: () => Promise<Connection[]>;
  addFolderConnection: (path: string) => Promise<Connection>;
  listSources: () => Promise<Source[]>;
  addSource: (source: NewSource) => Promise<Source>;
  getAgentDefinition: (agent: string) => Promise<ResolvedAgent | null>;
  setAgentOverride: (agent: string, field: string, value: unknown) => Promise<ResolvedAgent>;
}

function samePath(a: string, b: string): boolean {
  const trim = (path: string) => path.replace(/[\\/]+$/, "");
  return trim(a) === trim(b);
}

/**
 * Makes the folder at `folder` a source the agent `agent` consults: the
 * folder becomes a connection, the connection a source of the registry, and
 * the source is checked for the agent. Each step reuses what already exists,
 * so playing it twice adds nothing twice. Rejects when a step is refused, or
 * when the agent is not installed.
 */
export async function addFolderAsSource(steps: SourceSteps, agent: string, folder: string): Promise<Source> {
  const connections = await steps.listConnections();
  const connection =
    connections.find((c) => c.kind === "folder" && c.path != null && samePath(c.path, folder)) ??
    (await steps.addFolderConnection(folder));
  const registry = await steps.listSources();
  const source =
    registry.find((s) => s.kind === "folder" && s.reference === connection.id) ??
    (await steps.addSource({ kind: "folder", label: connection.label, reference: connection.id }));
  const definition = await steps.getAgentDefinition(agent);
  if (definition === null) throw new Error(`agent ${agent} is not installed`);
  if (!definition.sources.includes(source.id)) {
    await steps.setAgentOverride(agent, "sources", [...definition.sources, source.id]);
  }
  return source;
}
