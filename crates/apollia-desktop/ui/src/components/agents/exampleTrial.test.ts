import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import type { Connection } from "$lib/types";
import type { ResolvedAgent } from "$lib/ipc/agentSheet";
import type { NewSource, Source } from "$lib/ipc/sources";
import type { ExampleCard } from "./agentExamples";
import {
  addFolderAsSource,
  copyRefusal,
  hasDataset,
  trialAfterInstall,
  trialStart,
  type SourceSteps,
} from "./exampleTrial";

const QUALIFY = "qualify-requests";
const FOLDER = "/Users/demo/Documents/Apollia, example/referentiel";

function card(over: Partial<ExampleCard> = {}): ExampleCard {
  return {
    name: QUALIFY,
    title: "Qualify requests",
    mission: "Qualifies each request of a batch.",
    requires: [],
    installed: false,
    sheetAgent: QUALIFY,
    ...over,
  };
}

function source(file: string): string {
  return readFileSync(fileURLToPath(new URL(file, import.meta.url)), "utf8");
}

function connection(over: Partial<Connection> = {}): Connection {
  return {
    id: "folder:ref",
    kind: "folder",
    label: "referentiel",
    account: null,
    path: FOLDER,
    provenance: "user",
    remote: false,
    official: false,
    state: "connected",
    tools: [],
    ...over,
  } as Connection;
}

function registered(over: Partial<Source> = {}): Source {
  return {
    id: "source:ref",
    kind: "folder",
    label: "referentiel",
    group: null,
    reference: "folder:ref",
    indexState: "absent",
    addedAt: "2026-10-06T00:00:00Z",
    ...over,
  };
}

function agent(sources: string[]): ResolvedAgent {
  return {
    id: QUALIFY,
    origin: "package",
    package: QUALIFY,
    version: "1.0.0",
    enabled: true,
    name: "Qualify requests",
    mission: "",
    examples: [],
    access: [],
    sources,
    model: null,
    autonomy: "assisted",
    profile: null,
    overridden: [],
  };
}

/** The steps over a machine in a given state, recording every write. */
function steps(state: { connections: Connection[]; sources: Source[]; agent: ResolvedAgent | null }) {
  const writes: string[] = [];
  const calls: SourceSteps = {
    listConnections: async () => state.connections,
    addFolderConnection: async (path: string) => {
      writes.push(`connection ${path}`);
      return connection({ path });
    },
    listSources: async () => state.sources,
    addSource: async (wanted: NewSource) => {
      writes.push(`source ${wanted.kind} ${wanted.reference}`);
      return registered({ reference: wanted.reference, label: wanted.label });
    },
    getAgentDefinition: async () => state.agent,
    setAgentOverride: async (name: string, field: string, value: unknown) => {
      writes.push(`override ${name} ${field} ${JSON.stringify(value)}`);
      return agent(value as string[]);
    },
  };
  return { calls, writes };
}

describe("which example can be tried", () => {
  it("offers the trial on the example that comes with a dataset, and on no other", () => {
    // GIVEN the examples the application proposes
    // WHEN each is asked for a dataset
    // THEN only "qualify requests" has one
    expect(hasDataset(QUALIFY)).toBe(true);
    expect(hasDataset("veille-web")).toBe(false);
  });

  it("shows the trial button on the cards with a dataset only", () => {
    // GIVEN the cards of the Examples section
    const cards = source("./AgentExamples.svelte");

    // WHEN the template is read
    // THEN the button is rendered under the dataset condition, before and
    // after the installation alike
    const button = cards.indexOf('data-testid="agents-example-try"');
    const condition = cards.indexOf("{#if hasDataset(card.name)}");
    const installed = cards.indexOf("{#if !card.installed}");
    expect(condition).toBeGreaterThan(-1);
    expect(button).toBeGreaterThan(condition);
    expect(button).toBeLessThan(installed);
  });
});

describe("what the gesture does first", () => {
  it("opens the installation preview first on an example not installed", () => {
    // GIVEN the card of the example, not installed
    // WHEN the user tries it
    const start = trialStart(card({ installed: false }));

    // THEN the preview of the package comes first, and nothing is copied
    expect(start).toEqual({ kind: "install", example: QUALIFY });
  });

  it("opens the copy dialog at once on an installed example", () => {
    // GIVEN the card of the example, installed
    // WHEN the user tries it
    const start = trialStart(card({ installed: true }));

    // THEN the copy dialog opens on the agent the package brought
    expect(start).toEqual({ kind: "copy", trial: { example: QUALIFY, agent: QUALIFY } });
  });

  it("does nothing on an example without a dataset or without an agent", () => {
    // GIVEN a card without a dataset, and one without an agent
    // WHEN each is tried
    // THEN there is nothing to try
    expect(trialStart(card({ name: "veille-web" }))).toEqual({ kind: "none" });
    expect(trialStart(card({ installed: true, sheetAgent: null }))).toEqual({ kind: "none" });
  });
});

describe("what follows the installation a trial asked for", () => {
  it("opens the trial on the agent the awaited package brought", () => {
    // GIVEN a trial waiting for the installation of the example
    // WHEN that package is installed
    const next = trialAfterInstall(QUALIFY, { name: QUALIFY, agents: [{ id: "qualifier", name: "Qualifier" }] });

    // THEN the trial opens on its first agent
    expect(next).toEqual({ example: QUALIFY, agent: "qualifier" });
  });

  it("opens nothing for another package, without a wait, or without an agent", () => {
    // GIVEN installations that are not the awaited one
    // WHEN each is announced
    // THEN no trial opens
    expect(trialAfterInstall(QUALIFY, { name: "veille-web", agents: [{ id: "v", name: "V" }] })).toBeNull();
    expect(trialAfterInstall(null, { name: QUALIFY, agents: [{ id: "q", name: "Q" }] })).toBeNull();
    expect(trialAfterInstall(QUALIFY, { name: QUALIFY, agents: [] })).toBeNull();
    expect(trialAfterInstall(QUALIFY, null)).toBeNull();
  });
});

describe("nothing is copied without the gesture", () => {
  it("refuses the copy without a destination, and while one is running", () => {
    // GIVEN the dialog before the copy
    // WHEN the destination is blank, then set, then a copy is running
    // THEN only a set destination with no copy under way may be copied into
    expect(copyRefusal("   ", false)).toBe("destination_missing");
    expect(copyRefusal("/Users/demo/Documents", false)).toBeNull();
    expect(copyRefusal("/Users/demo/Documents", true)).toBe("copying");
  });

  it("copies from one place only: the confirmation button of the dialog", () => {
    // GIVEN the dialog and the screen that opens it
    const dialog = source("./ExampleTrialDialog.svelte");
    const route = source("../../routes/Agents.svelte");
    const cards = source("./AgentExamples.svelte");

    // WHEN their calls are read
    const calls = dialog.match(/copyExampleDataset\(/g) ?? [];
    const copy = dialog.slice(dialog.indexOf("async function copy()"), dialog.indexOf("async function launch()"));

    // THEN the copy is called once, inside the handler of "Copy the example",
    // behind the refusal; opening the dialog, the screen and the cards call
    // nothing that copies
    expect(calls).toHaveLength(1);
    expect(copy).toContain("refusal !== null) return;");
    expect(copy).toContain("copyExampleDataset(trial.example, destination.trim())");
    expect(dialog).toContain('onclick={copy}\n        data-testid="example-trial-copy"');
    expect(route).not.toContain("copyExampleDataset");
    expect(cards).not.toContain("copyExampleDataset");
  });

  it("goes through the preview before the trial of an example not installed", () => {
    // GIVEN the screen
    const route = source("../../routes/Agents.svelte");

    // WHEN the handler of the trial button is read
    const handler = route.slice(route.indexOf("function tryExample("), route.indexOf("function launchTrialBatch("));

    // THEN an example to install opens its preview and waits; the copy dialog
    // opens from the installation, or at once when already installed
    expect(handler).toContain("openExamplePreview(start.example);");
    expect(handler).toContain("awaitedTrial = start.example;");
    expect(route).toContain("trialAfterInstall(awaitedTrial, installed)");
    expect(route).toContain("if ($packagePreviewRequest === null) awaitedTrial = null;");
  });
});

describe("the copied folder becomes a source of the agent", () => {
  it("adds the connection, the source and the check on a fresh machine", async () => {
    // GIVEN no folder connection, no source, and the agent with its profile
    const { calls, writes } = steps({ connections: [], sources: [], agent: agent(["profile"]) });

    // WHEN the folder is added as a source of the agent
    const added = await addFolderAsSource(calls, QUALIFY, FOLDER);

    // THEN the folder becomes a connection, the connection a source, and the
    // source is checked beside what the agent already consulted
    expect(added.reference).toBe("folder:ref");
    expect(writes).toEqual([
      `connection ${FOLDER}`,
      "source folder folder:ref",
      `override ${QUALIFY} sources ["profile","source:ref"]`,
    ]);
  });

  it("adds nothing twice when the folder is already a checked source", async () => {
    // GIVEN the folder already a connection, a source, and checked
    const { calls, writes } = steps({
      connections: [connection({ path: `${FOLDER}/` })],
      sources: [registered()],
      agent: agent(["source:ref"]),
    });

    // WHEN the folder is added again
    const added = await addFolderAsSource(calls, QUALIFY, FOLDER);

    // THEN the existing source is answered and nothing is written
    expect(added.id).toBe("source:ref");
    expect(writes).toEqual([]);
  });

  it("refuses an agent that is not installed, after the source is registered", async () => {
    // GIVEN no definition for the agent
    const { calls, writes } = steps({ connections: [], sources: [], agent: null });

    // WHEN the folder is added as its source
    const refused = addFolderAsSource(calls, QUALIFY, FOLDER);

    // THEN it rejects, and no agent is written to
    await expect(refused).rejects.toThrow("is not installed");
    expect(writes.some((write) => write.startsWith("override"))).toBe(false);
  });
});

describe("after the copy", () => {
  it("proposes the folder as a source, checked, and the workbook as a batch", () => {
    // GIVEN the dialog and the screen
    const dialog = source("./ExampleTrialDialog.svelte");
    const route = source("../../routes/Agents.svelte");

    // WHEN the copied stage is read
    // THEN the source is a checkbox checked by default, "Later" leaves, and
    // the launch hands the agent and the copied workbook to the launch dialog
    expect(dialog).toContain("let alsoSource = $state(true);");
    expect(dialog).toContain('data-testid="example-trial-also-source"');
    expect(dialog).toContain('data-testid="example-trial-later"');
    expect(dialog).toContain("onLaunchBatch(agent, workbook);");
    expect(route).toContain("file={trialBatch?.workbook ?? null}");
    expect(route).toContain("agent={trialBatch?.agent ?? null}");
  });
});
