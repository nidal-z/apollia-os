/**
 * The decisions of the "first agent" step of the first launch.
 *
 * The step offers three ways to an agent: install a package file, create one
 * by describing the need, or start from an example the application proposes.
 * Whichever way is taken, the agent obtained is kept for the two steps that
 * follow (`$lib/onboarding/firstAgent`); a step that is passed keeps nothing.
 *
 * Kept free of Svelte so each decision is tested on its own.
 */
import type { ExamplePackage } from "$lib/ipc/packageExamples";
import type { FirstAgent, FirstAgentWay } from "$lib/onboarding/firstAgent";
import { exampleCards, type ExampleCard } from "../agents/agentExamples";

/** What a card of the step opens. */
export type FirstAgentOpening =
  /** The file chooser, then the preview of the chosen package. */
  | "package-preview"
  /** The conversation that drafts an agent, inside the step. */
  | "create-conversation"
  /** The list of the examples, inside the step. */
  | "example-list";

export interface FirstAgentCard {
  way: FirstAgentWay;
  opens: FirstAgentOpening;
  titleKey: string;
  bodyKey: string;
}

/** The three cards, in the order the step shows them. */
export const FIRST_AGENT_CARDS: readonly FirstAgentCard[] = [
  {
    way: "install",
    opens: "package-preview",
    titleKey: "onboarding.first_agent.install_title",
    bodyKey: "onboarding.first_agent.install_body",
  },
  {
    way: "create",
    opens: "create-conversation",
    titleKey: "onboarding.first_agent.create_title",
    bodyKey: "onboarding.first_agent.create_body",
  },
  {
    way: "example",
    opens: "example-list",
    titleKey: "onboarding.first_agent.example_title",
    bodyKey: "onboarding.first_agent.example_body",
  },
];

/** What the step shows: its cards, or what one of them opened in place. */
export type FirstAgentView = "cards" | "create" | "examples";

/** The view a card opens in the step; the install card opens a dialog above it. */
export function viewOfCard(card: FirstAgentCard): FirstAgentView {
  if (card.opens === "create-conversation") return "create";
  if (card.opens === "example-list") return "examples";
  return "cards";
}

/** An agent a package brings, as the installed event names it. */
export interface InstalledAgent {
  id: string;
  name: string;
}

/** What the preview says once a package is installed. */
export interface InstalledPackage {
  /** Package identifier. */
  name: string;
  /** The agents it brought, in the order of the package. */
  agents: InstalledAgent[];
}

/**
 * The agent kept after the package `installed` was installed by `way`: its
 * first agent. A package that brings no agent keeps nothing, and the step
 * says so instead of moving on.
 */
export function agentOfPackage(
  installed: InstalledPackage,
  way: Exclude<FirstAgentWay, "create">,
): FirstAgent | null {
  const agent = installed.agents[0];
  if (agent === undefined) return null;
  return {
    id: agent.id,
    name: agent.name.trim() || agent.id,
    way,
    packageName: installed.name,
  };
}

/** The agent kept after it was created in the step. */
export function agentCreated(agent: InstalledAgent): FirstAgent {
  return { id: agent.id, name: agent.name.trim() || agent.id, way: "create", packageName: null };
}

/**
 * Reads the detail of the installed event. Anything that is not the shape the
 * preview sends answers `null`, and the step then keeps nothing.
 */
export function installedPackageOf(detail: unknown): InstalledPackage | null {
  if (typeof detail !== "object" || detail === null) return null;
  const { name, agents } = detail as { name?: unknown; agents?: unknown };
  if (typeof name !== "string" || name.length === 0) return null;
  const listed: InstalledAgent[] = [];
  for (const entry of Array.isArray(agents) ? agents : []) {
    if (typeof entry !== "object" || entry === null) continue;
    const { id, name: agentName } = entry as { id?: unknown; name?: unknown };
    if (typeof id !== "string" || id.length === 0) continue;
    listed.push({ id, name: typeof agentName === "string" ? agentName : id });
  }
  return { name, agents: listed };
}

/**
 * The way a package installed while the step is open was obtained: the
 * example the step asked the preview for, else a file. A package installed
 * under another name than the example asked for is a file, whatever the list
 * was showing.
 */
export function wayOfInstalled(
  installed: InstalledPackage,
  askedExample: string | null,
): Exclude<FirstAgentWay, "create"> {
  return askedExample !== null && installed.name === askedExample ? "example" : "install";
}

/** One line of the list of examples. */
export interface FirstAgentExample extends ExampleCard {
  /** The line the person chose; one at most. */
  chosen: boolean;
}

/**
 * The lines of the list of examples, the one named `chosen` marked. A single
 * example is chosen at a time: it is the only one the step asks to install.
 */
export function exampleLines(
  examples: ExamplePackage[],
  chosen: string | null,
): FirstAgentExample[] {
  return exampleCards(examples).map((card) => ({ ...card, chosen: card.name === chosen }));
}

/**
 * The example to install for the choice `chosen`: its package name, and that
 * one only. `null` while nothing is chosen, or when the choice names an
 * example the list does not hold.
 */
export function exampleToInstall(examples: ExamplePackage[], chosen: string | null): string | null {
  if (chosen === null) return null;
  return examples.some((example) => example.name === chosen) ? chosen : null;
}

/**
 * The agent kept when the chosen example is already installed: nothing is
 * installed again, its first agent is the one the next steps work with.
 */
export function agentOfInstalledExample(
  examples: ExamplePackage[],
  chosen: string | null,
): FirstAgent | null {
  const example = examples.find((entry) => entry.name === chosen);
  if (example === undefined || (example.installedVersion ?? null) === null) return null;
  return agentOfPackage({ name: example.name, agents: example.agents }, "example");
}
