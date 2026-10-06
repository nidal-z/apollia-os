import { describe, expect, it } from "vitest";
import type { ExamplePackage } from "$lib/ipc/packageExamples";
import { exampleCard, exampleCards, examplesOpenByDefault } from "./agentExamples";

function example(over: Partial<ExamplePackage> = {}): ExamplePackage {
  return {
    name: "veille-web",
    version: "1.0.0",
    description: "Watches a topic on the web.",
    agents: [{ id: "veille-web", name: "Veille web", mission: "  Search the web on your topic.  " }],
    requires: [],
    installedVersion: null,
    ...over,
  };
}

describe("exampleCard", () => {
  it("shows the name and the mission of the first agent", () => {
    // GIVEN an example with one agent
    // WHEN its card is built
    const card = exampleCard(example());

    // THEN the card carries the agent's name and its mission, trimmed
    expect(card.title).toBe("Veille web");
    expect(card.mission).toBe("Search the web on your topic.");
    expect(card.name).toBe("veille-web");
  });

  it("falls back on the package when it declares no agent", () => {
    // GIVEN an example without an agent
    // WHEN its card is built
    const card = exampleCard(example({ agents: [] }));

    // THEN the package names it and describes it, and no sheet can open
    expect(card.title).toBe("veille-web");
    expect(card.mission).toBe("Watches a topic on the web.");
    expect(card.sheetAgent).toBeNull();
  });

  it("lists each required connection once", () => {
    // GIVEN an example that requires a mailbox twice and a folder
    const requires = [
      { type: "imap", reason: "Reads the mails", present: false, broughtByPackage: false },
      { type: "folder", reason: "Reads the documents", present: true, broughtByPackage: false },
      { type: "imap", reason: "Reads the newsletters", present: false, broughtByPackage: false },
    ];

    // WHEN its card is built
    const card = exampleCard(example({ requires }));

    // THEN each type is named once, in order
    expect(card.requires).toEqual(["imap", "folder"]);
  });

  it("is proposed until the user installed it", () => {
    // GIVEN the same example, proposed then installed
    // WHEN both cards are built
    const proposed = exampleCard(example());
    const installed = exampleCard(example({ installedVersion: "1.0.0" }));

    // THEN only the second reads installed, and leads to the agent's sheet
    expect(proposed.installed).toBe(false);
    expect(installed.installed).toBe(true);
    expect(installed.sheetAgent).toBe("veille-web");
  });
});

describe("exampleCards", () => {
  it("keeps the order of the runtime", () => {
    // GIVEN two examples
    const listed = [example({ name: "b" }), example({ name: "a" })];

    // WHEN their cards are built
    // THEN they come in the same order
    expect(exampleCards(listed).map((card) => card.name)).toEqual(["b", "a"]);
  });
});

describe("examplesOpenByDefault", () => {
  it("opens the cards on a screen with nothing else", () => {
    // GIVEN no agent, no workflow and no filter
    // WHEN the section decides
    // THEN it opens on its cards
    expect(examplesOpenByDefault(0, 0, false)).toBe(true);
  });

  it("stays a band beside agents or workflows", () => {
    // GIVEN a screen with an agent, or with a workflow
    // WHEN the section decides
    // THEN it stays a band
    expect(examplesOpenByDefault(2, 0, false)).toBe(false);
    expect(examplesOpenByDefault(0, 1, false)).toBe(false);
  });

  it("stays a band when a filter emptied the lists", () => {
    // GIVEN lists a filter emptied
    // WHEN the section decides
    // THEN an empty result is not an empty screen
    expect(examplesOpenByDefault(0, 0, true)).toBe(false);
  });
});
