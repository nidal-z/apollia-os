import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, test, expect } from "vitest";
import en from "$lib/i18n/en.json";
import fr from "$lib/i18n/fr.json";
import type { ExamplePackage } from "$lib/ipc/packageExamples";
import {
  FIRST_AGENT_CARDS,
  agentCreated,
  agentOfInstalledExample,
  agentOfPackage,
  exampleLines,
  exampleToInstall,
  installedPackageOf,
  viewOfCard,
  wayOfInstalled,
  type FirstAgentCard,
} from "./firstAgent";

// Vitest runs in `node`: the step is not mounted here, its gestures are played
// by the automation recipes. These tests hold the decisions of the step, and
// read its template for what each card is wired to.

function source(file: string): string {
  return readFileSync(fileURLToPath(new URL(file, import.meta.url)), "utf8");
}

function card(way: FirstAgentCard["way"]): FirstAgentCard {
  const found = FIRST_AGENT_CARDS.find((entry) => entry.way === way);
  expect(found, way).toBeDefined();
  return found as FirstAgentCard;
}

function example(name: string, over: Partial<ExamplePackage> = {}): ExamplePackage {
  return {
    name,
    version: "1.0.0",
    description: `Description of ${name}`,
    agents: [{ id: `${name}-agent`, name: `Agent of ${name}`, mission: `Mission of ${name}` }],
    requires: [],
    installedVersion: null,
    ...over,
  };
}

const EXAMPLES: ExamplePackage[] = [
  example("web-watch"),
  example("mail-triage", {
    requires: [{ type: "imap", reason: "reads the inbox", present: false, broughtByPackage: false }],
  }),
  example("document-analysis"),
  example("qualify-requests"),
];

const step = source("./OnboardingFirstAgent.svelte");

describe("first agent - the three cards", () => {
  test("the step shows three cards, in the order install, create, example", () => {
    // GIVEN the cards of the step
    // WHEN their ways are read
    // THEN there are three, and the template draws this list and no other
    expect(FIRST_AGENT_CARDS.map((entry) => entry.way)).toEqual(["install", "create", "example"]);
    expect(step).toContain("{#each FIRST_AGENT_CARDS as card (card.way)}");
    expect(step).toContain("data-testid={`first-agent-card-${card.way}`}");
  });

  test("the install card opens the preview dialog on the chosen package file", () => {
    // GIVEN the card "Install an agent"
    const install = card("install");

    // WHEN what it opens is read
    // THEN it is the preview of a package, and the step stays on its cards
    expect(install.opens).toBe("package-preview");
    expect(viewOfCard(install)).toBe("cards");
    // AND the template asks for a `.apo` file, then hands its path to the
    // preview mounted above every screen, which installs nothing by itself
    expect(step).toContain('if (card.opens === "package-preview") {');
    expect(step).toContain('filters: [{ name: "Apollia", extensions: ["apo"] }]');
    expect(step).toContain("openPackagePreview(path);");
    expect(step).not.toContain("installPackageFile");
  });

  test("the create card opens the conversation that drafts an agent, inside the step", () => {
    // GIVEN the card "Create by describing my need"
    const create = card("create");

    // WHEN what it opens is read
    // THEN it is the creation conversation, shown in place of the cards
    expect(create.opens).toBe("create-conversation");
    expect(viewOfCard(create)).toBe("create");
    // AND the template mounts the creation of the Agents screen, embedded,
    // and keeps the agent it saves
    expect(step).toContain('{#if view === "create"}');
    expect(step).toContain("<CreateAgent");
    expect(step).toContain("embedded={true}");
    expect(step).toContain("onboardingFirstAgent.retain(agentCreated(agent));");
  });

  test("the example card lists the examples the application proposes", () => {
    // GIVEN the card "Start from an example"
    const fromExample = card("example");

    // WHEN what it opens is read
    // THEN it is the list of examples, shown in place of the cards
    expect(fromExample.opens).toBe("example-list");
    expect(viewOfCard(fromExample)).toBe("examples");
    // AND the template reads the list from the runtime and draws one line each
    expect(step).toContain('if (view === "examples") void loadExamples();');
    expect(step).toContain("examples = await listExamplePackages();");
    expect(step).toContain("{#each lines as line (line.name)}");
    // AND the lines are those of the examples, in the order of the runtime
    expect(exampleLines(EXAMPLES, null).map((line) => line.name)).toEqual([
      "web-watch",
      "mail-triage",
      "document-analysis",
      "qualify-requests",
    ]);
  });

  test("every card has its title and its sentence in both languages", () => {
    // GIVEN the two catalogues
    for (const catalogue of [fr, en]) {
      const texts = catalogue.onboarding.first_agent as Record<string, string>;

      // WHEN the texts of each card are looked up
      // THEN each is there and says something
      for (const entry of FIRST_AGENT_CARDS) {
        for (const key of [entry.titleKey, entry.bodyKey]) {
          expect(texts[key.replace("onboarding.first_agent.", "")]?.trim(), key).toBeTruthy();
        }
      }
    }
  });

  test("every text the step shows exists in both languages", () => {
    // GIVEN the keys the template and its cards read
    const keys = new Set(
      [...step.matchAll(/"onboarding\.first_agent\.([a-z_]+)"/g)].map((match) => match[1]),
    );
    for (const entry of FIRST_AGENT_CARDS) {
      keys.add(entry.titleKey.replace("onboarding.first_agent.", ""));
      keys.add(entry.bodyKey.replace("onboarding.first_agent.", ""));
    }

    // WHEN each is looked up
    // THEN each is in the two catalogues, and the catalogues hold no other
    expect(keys.size).toBeGreaterThan(0);
    for (const catalogue of [fr, en]) {
      expect(Object.keys(catalogue.onboarding.first_agent).sort()).toEqual([...keys].sort());
    }
  });
});

describe("first agent - the agent a package brings is kept", () => {
  test("a package installed from a file keeps its first agent", () => {
    // GIVEN the detail the preview sends once a package is installed
    const installed = installedPackageOf({
      name: "press-pack",
      agents: [
        { id: "press-watch", name: "Press watch" },
        { id: "press-digest", name: "Press digest" },
      ],
    });

    // WHEN the step reads it, no example having been asked for
    expect(installed).not.toBeNull();
    if (installed === null) return;
    const agent = agentOfPackage(installed, wayOfInstalled(installed, null));

    // THEN the first agent of the package is kept, with the package it came from
    expect(agent).toEqual({
      id: "press-watch",
      name: "Press watch",
      way: "install",
      packageName: "press-pack",
    });
  });

  test("a package that brings no agent keeps none", () => {
    // GIVEN a package of tools only
    const installed = installedPackageOf({ name: "tools-only", agents: [] });

    // WHEN the step reads it
    // THEN no agent is kept, and the template says so instead of moving on
    expect(installed).toEqual({ name: "tools-only", agents: [] });
    expect(agentOfPackage({ name: "tools-only", agents: [] }, "install")).toBeNull();
    expect(step).toContain('"onboarding.first_agent.no_agent_in_package"');
  });

  test("a detail that is not the one of the preview is ignored", () => {
    // GIVEN details the event should never carry
    // WHEN each is read
    // THEN none is a package
    for (const detail of [null, undefined, "press-pack", {}, { name: "" }, { agents: [] }]) {
      expect(installedPackageOf(detail), JSON.stringify(detail)).toBeNull();
    }
    // AND a package named without its agents is one that brings none
    expect(installedPackageOf({ name: "old-shape" })).toEqual({ name: "old-shape", agents: [] });
  });

  test("the preview names the agents of the package it installed", () => {
    // GIVEN the preview dialog every package goes through
    const preview = source("../packages/PackagePreviewDialog.svelte");

    // WHEN its installed event is read
    // THEN it carries the name of the package and the agents it brought
    expect(preview).toContain("const agents = preview.agents.map(({ id, name }) => ({ id, name }));");
    expect(preview).toContain("detail: { name: preview.name, agents }");
  });

  test("an agent created in the step is kept, with no package", () => {
    // GIVEN the agent the creation saved
    // WHEN the step keeps it
    // THEN it is kept as created, brought by no package
    expect(agentCreated({ id: "notes-helper", name: " Notes helper " })).toEqual({
      id: "notes-helper",
      name: "Notes helper",
      way: "create",
      packageName: null,
    });
    expect(agentCreated({ id: "nameless", name: "" }).name).toBe("nameless");
  });
});

describe("first agent - an example is installed only when chosen, and alone", () => {
  test("nothing is chosen when the list opens, and nothing can be installed", () => {
    // GIVEN the list of examples as it opens
    const lines = exampleLines(EXAMPLES, null);

    // WHEN its choice is read
    // THEN no line is chosen and there is no example to install
    expect(lines.some((line) => line.chosen)).toBe(false);
    expect(exampleToInstall(EXAMPLES, null)).toBeNull();
    expect(step).toContain("disabled={installable === null}");
  });

  test("choosing one example marks that line and no other", () => {
    // GIVEN the list of examples
    // WHEN "qualify-requests" is chosen
    const lines = exampleLines(EXAMPLES, "qualify-requests");

    // THEN it is the only line chosen
    expect(lines.filter((line) => line.chosen).map((line) => line.name)).toEqual(["qualify-requests"]);
  });

  test("the example asked of the preview is the chosen one, and that one only", () => {
    // GIVEN "qualify-requests" chosen among four examples
    // WHEN the example to install is read
    // THEN it is that package name alone: one string, never a list
    expect(exampleToInstall(EXAMPLES, "qualify-requests")).toBe("qualify-requests");
    // AND the template hands exactly that name to the preview, once
    expect(step.match(/openExamplePreview\(/g)).toHaveLength(1);
    expect(step).toContain("openExamplePreview(installable);");
    expect(step).not.toContain("installExamplePackage");
  });

  test("a choice the list does not hold installs nothing", () => {
    // GIVEN a choice left over from a list that changed
    // WHEN the example to install is read
    // THEN there is none
    expect(exampleToInstall(EXAMPLES, "gone")).toBeNull();
    expect(exampleToInstall([], "qualify-requests")).toBeNull();
  });

  test("the installed example keeps its agent, as an example", () => {
    // GIVEN the example the step asked the preview for, then installed
    const installed = installedPackageOf({
      name: "qualify-requests",
      agents: [{ id: "qualify-requests-agent", name: "Request qualifier" }],
    });
    expect(installed).not.toBeNull();
    if (installed === null) return;

    // WHEN the step reads the installed event
    const way = wayOfInstalled(installed, "qualify-requests");

    // THEN its agent is kept as coming from an example
    expect(way).toBe("example");
    expect(agentOfPackage(installed, way)).toEqual({
      id: "qualify-requests-agent",
      name: "Request qualifier",
      way: "example",
      packageName: "qualify-requests",
    });
    // AND a package installed under another name meanwhile is a file
    expect(wayOfInstalled({ name: "press-pack", agents: [] }, "qualify-requests")).toBe("install");
  });

  test("an example already installed is kept without being installed again", () => {
    // GIVEN a list where the chosen example is already installed
    const examples = [example("web-watch", { installedVersion: "1.0.0" }), example("mail-triage")];

    // WHEN its agent is asked for
    // THEN it is kept as it is, and an example not installed yet gives none
    expect(agentOfInstalledExample(examples, "web-watch")).toEqual({
      id: "web-watch-agent",
      name: "Agent of web-watch",
      way: "example",
      packageName: "web-watch",
    });
    expect(agentOfInstalledExample(examples, "mail-triage")).toBeNull();
    expect(agentOfInstalledExample(examples, null)).toBeNull();
  });

  test("a line says what its example requires", () => {
    // GIVEN an example that requires a connection
    // WHEN its line is read
    // THEN the type of the connection is on it
    const line = exampleLines(EXAMPLES, null).find((entry) => entry.name === "mail-triage");
    expect(line?.requires).toEqual(["imap"]);
    expect(step).toContain('data-testid="first-agent-example-requires"');
  });
});

describe("first agent - the step can be passed", () => {
  test("skip keeps no agent and moves on", () => {
    // GIVEN the template of the step
    // WHEN its skip is read
    // THEN it clears the agent kept, then hands over to the next step
    expect(step).toContain('data-testid="first-agent-skip"');
    expect(step).toMatch(/function skip\(\): void \{\s+onboardingFirstAgent\.clear\(\);\s+onnext\(\);\s+\}/);
  });

  test("once an agent is kept, the step offers to continue with it", () => {
    // GIVEN the template of the step
    // WHEN its footer is read
    // THEN "Continue" moves on without clearing the agent, and the agent kept is named
    expect(step).toContain('data-testid="first-agent-continue"');
    expect(step).toContain('data-testid="first-agent-kept"');
    expect(step).toContain("onclick={onnext}");
  });

  test("a first launch replayed from before the step keeps no agent of an earlier run", () => {
    // GIVEN the source of the modal
    const modal = source("./OnboardingModal.svelte");

    // WHEN the restoration of its step is read
    // THEN a phase before the first agent clears what an earlier run kept
    expect(modal).toContain(
      'ONBOARDING_PHASE_ORDER.indexOf(state.phase) < ONBOARDING_PHASE_ORDER.indexOf("first_agent")',
    );
    expect(modal).toContain("onboardingFirstAgent.clear();");
  });
});

describe("first agent - dialogs opened from the first launch are not painted under it", () => {
  test("a dialog rises above the veil while the first launch is shown", () => {
    // GIVEN the dialog primitive the preview of a package is built on
    const dialog = source("../../lib/components/ui/dialog/Dialog.svelte");

    // WHEN its layer is read
    // THEN it sits above the veil while the first launch is open, and on the
    // usual layers otherwise
    expect(dialog).toContain("$onboardingModalOpen");
    expect(dialog).toContain("{ backdrop: zLayer.onboardingAbove, dialog: zLayer.onboardingAbove }");
    expect(dialog).toContain("{ backdrop: zLayer.backdrop, dialog: zLayer.overlay }");
  });
});
