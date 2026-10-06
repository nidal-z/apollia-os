/**
 * What the Examples section of the Agents screen shows, decided from the
 * runtime's list.
 *
 * The section renders this and nothing else: the name and the mission of each
 * example, what it requires, whether the user installed it and which sheet
 * "Open the sheet" leads to, and whether the section opens on its cards or
 * stays a band. Kept free of Svelte so the decisions are tested on their own.
 */
import type { ExamplePackage } from "$lib/ipc/packageExamples";

/** One card of the section. */
export interface ExampleCard {
  /** Package identifier: what the preview and the installation name. */
  name: string;
  /** Name shown: the one of its first agent, else the package identifier. */
  title: string;
  /** What it does: the mission of its first agent, else the description. */
  mission: string;
  /** The connections it requires, by type, each once. */
  requires: string[];
  installed: boolean;
  /** The agent whose sheet an installed example opens, `null` without one. */
  sheetAgent: string | null;
}

/** The card of `example`. */
export function exampleCard(example: ExamplePackage): ExampleCard {
  const agent = example.agents[0];
  const mission = agent?.mission.trim() ?? "";
  return {
    name: example.name,
    title: agent?.name.trim() || example.name,
    mission: mission.length > 0 ? mission : example.description.trim(),
    requires: [...new Set(example.requires.map((required) => required.type))],
    installed: (example.installedVersion ?? null) !== null,
    sheetAgent: agent?.id ?? null,
  };
}

/** The cards of `examples`, in the order the runtime lists them. */
export function exampleCards(examples: ExamplePackage[]): ExampleCard[] {
  return examples.map(exampleCard);
}

/**
 * The section opens on its cards when the screen has nothing else to show:
 * no agent, no workflow, and no filter that could have hidden them. Otherwise
 * it stays a band until the user opens it.
 */
export function examplesOpenByDefault(
  agentCount: number,
  workflowCount: number,
  hasQuery: boolean,
): boolean {
  return !hasQuery && agentCount === 0 && workflowCount === 0;
}
