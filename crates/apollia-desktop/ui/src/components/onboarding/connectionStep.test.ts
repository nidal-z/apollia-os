import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, test, expect } from "vitest";
import en from "$lib/i18n/en.json";
import fr from "$lib/i18n/fr.json";
import type { FirstAgent } from "$lib/onboarding/firstAgent";
import type { Connection, RegistryServerView } from "$lib/types";
import {
  allPresent,
  connectionType,
  entryToAdd,
  loadConnectionProposal,
  missingLines,
  nothingToPropose,
  refreshedLines,
  requiredConnections,
  subtitleKey,
  type ConnectionSources,
} from "./connectionStep";
import { nextStep } from "./onboardingSteps";

// Vitest runs in `node`: the step is not mounted here, its gestures are
// played by the automation recipes. These tests hold its decisions, and read
// the templates for what must be wired in them.

function source(file: string): string {
  return readFileSync(fileURLToPath(new URL(file, import.meta.url)), "utf8");
}

const AGENT: FirstAgent = {
  id: "request-sorter",
  name: "Request sorter",
  way: "example",
  packageName: "request-sorter",
};

const BROWSER_ENTRY = {
  name: "@modelcontextprotocol/server-puppeteer",
  title: "Web Browser",
  packages: [{ identifier: "@modelcontextprotocol/server-puppeteer" }],
  enrichment: { operator_label: "Web Browser" },
} as unknown as RegistryServerView;

const NOTION_ENTRY = {
  name: "com.notion/mcp",
  title: "Notion",
  packages: null,
  enrichment: null,
} as unknown as RegistryServerView;

const MANIFEST = {
  package: { name: "request-sorter" },
  requires: [
    { type: "@modelcontextprotocol/server-puppeteer", reason: "Opens the pages the requests cite." },
    { type: "com.notion/mcp", reason: "Files each request." },
  ],
};

function connection(id: string, kind: Connection["kind"]): Connection {
  return { id, kind } as Connection;
}

function sources(over: Partial<ConnectionSources> = {}): ConnectionSources {
  return {
    manifest: async () => MANIFEST,
    connections: async () => [],
    selection: async () => [BROWSER_ENTRY, NOTION_ENTRY],
    ...over,
  };
}

describe("connection step - what the package requires", () => {
  test("each required connection is read once, with its reason", () => {
    // GIVEN a recorded manifest that names a connection twice
    const manifest = {
      requires: [
        { type: "imap", reason: "Reads the inbox." },
        { type: "IMAP", reason: "Again." },
        { type: "folder" },
      ],
    };

    // WHEN what it requires is read
    // THEN each type comes once, a missing reason is an empty one
    expect(requiredConnections(manifest)).toEqual([
      { type: "imap", reason: "Reads the inbox." },
      { type: "folder", reason: "" },
    ]);
  });

  test("a connection the package brings itself is not required of the person", () => {
    // GIVEN a manifest that requires a server it also brings
    const manifest = {
      requires: [{ type: "Tickets", reason: "Reads the tickets." }, { type: "imap" }],
      mcp_servers: [{ name: "tickets" }],
    };

    // WHEN what it requires is read
    // THEN only the connection the package does not bring is left
    expect(requiredConnections(manifest).map((r) => r.type)).toEqual(["imap"]);
  });

  test("a manifest that cannot be read requires nothing", () => {
    // GIVEN values that are not a manifest, or whose entries are malformed
    // WHEN each is read
    // THEN none requires anything, and none throws
    for (const manifest of [null, 42, "text", {}, { requires: "imap" }, { requires: [7, { type: "" }, {}] }]) {
      expect(requiredConnections(manifest), JSON.stringify(manifest)).toEqual([]);
    }
  });

  test("the type of a connection is read the way a package names it", () => {
    // GIVEN one connection of each kind
    // WHEN its type is read
    // THEN a mailbox is imap, a folder is folder, the others the segment after their kind
    expect(connectionType(connection("imap:me@example.org", "imap"))).toBe("imap");
    expect(connectionType(connection("folder:/tmp/work", "folder"))).toBe("folder");
    expect(connectionType(connection("mcp:Notion", "mcp"))).toBe("notion");
    expect(connectionType(connection("native:google:me@example.org", "native"))).toBe("google");
  });
});

describe("connection step - the connections required and missing are listed", () => {
  test("the step lists what the chosen agent requires and this machine lacks", async () => {
    // GIVEN an agent taken from a package that requires two connections, none of them there
    // WHEN the step reads what to propose
    const proposal = await loadConnectionProposal(AGENT, sources());

    // THEN both are listed as missing, named as the selection names them, each on its entry
    expect(proposal?.agentName).toBe("Request sorter");
    expect(proposal?.lines).toEqual([
      {
        type: "@modelcontextprotocol/server-puppeteer",
        label: "Web Browser",
        reason: "Opens the pages the requests cite.",
        state: "missing",
        entry: BROWSER_ENTRY,
      },
      {
        type: "com.notion/mcp",
        label: "Notion",
        reason: "Files each request.",
        state: "missing",
        entry: NOTION_ENTRY,
      },
    ]);
  });

  test("the manifest read is the one of the package of the agent", async () => {
    // GIVEN the sources, watched
    const asked: string[] = [];
    const watched = sources({
      manifest: async (name) => {
        asked.push(name);
        return MANIFEST;
      },
    });

    // WHEN the step reads what to propose
    await loadConnectionProposal(AGENT, watched);

    // THEN the package of the agent kept was asked for, and no other
    expect(asked).toEqual(["request-sorter"]);
  });

  test("a connection already there is not listed", () => {
    // GIVEN two required connections, one added earlier under the name of its entry
    const required = requiredConnections(MANIFEST);
    const connections = [connection("mcp:web-browser", "mcp")];

    // WHEN the lines are built
    const lines = missingLines(required, connections, [BROWSER_ENTRY, NOTION_ENTRY]);

    // THEN only the one still missing is listed
    expect(lines.map((line) => line.type)).toEqual(["com.notion/mcp"]);
  });

  test("Add opens the add dialog on the entry of the line", () => {
    // GIVEN the lines of the step
    const lines = missingLines(requiredConnections(MANIFEST), [], [BROWSER_ENTRY, NOTION_ENTRY]);

    // WHEN the entry to add is asked for a line
    // THEN it is the selection entry of that line, and no other
    expect(entryToAdd(lines, "com.notion/mcp")).toBe(NOTION_ENTRY);
    expect(entryToAdd(lines, "@modelcontextprotocol/server-puppeteer")).toBe(BROWSER_ENTRY);
    expect(entryToAdd(lines, "unknown")).toBeNull();

    // AND the step hands that entry to the add dialog of the Connections screen
    const step = source("./OnboardingConnection.svelte");
    expect(step).toContain('import ConnectorWizard from "../integrations/ConnectorWizard.svelte";');
    expect(step).toContain("const addingEntry = $derived(adding === null ? null : entryToAdd(lines, adding));");
    expect(step).toContain("server={addingEntry}");
    expect(step).toContain("onclick={() => (adding = line.type)}");
    expect(step).toContain('data-testid="connection-line-add"');
  });

  test("a connection the selection does not hold is listed without Add", () => {
    // GIVEN a required connection no entry of the selection stands for
    const lines = missingLines([{ type: "imap", reason: "Reads the inbox." }], [], [NOTION_ENTRY]);

    // WHEN its line is read
    // THEN it is listed under its type, with no entry to open the dialog on
    expect(lines).toEqual([
      { type: "imap", label: "imap", reason: "Reads the inbox.", state: "missing", entry: null },
    ]);
    expect(entryToAdd(lines, "imap")).toBeNull();
    expect(source("./OnboardingConnection.svelte")).toContain('data-testid="connection-line-later"');
  });
});

describe("connection step - passed over when there is nothing to propose", () => {
  test("without an agent the step is passed over, and nothing is read", async () => {
    // GIVEN no agent kept, and sources that would fail if asked
    const refusing = sources({
      manifest: async () => {
        throw new Error("must not be asked");
      },
    });

    // WHEN the step reads what to propose
    const proposal = await loadConnectionProposal(null, refusing);

    // THEN there is nothing, and the step after the first agent is not this one
    expect(proposal).toBeNull();
    expect(nothingToPropose(proposal)).toBe(true);
    expect(nextStep("first-agent", ["connection"])).not.toBe("connection");
  });

  test("an agent created in the step comes from no package and is passed over", async () => {
    // GIVEN an agent created by describing the need
    const created: FirstAgent = { ...AGENT, way: "create", packageName: null };

    // WHEN the step reads what to propose
    // THEN there is nothing to propose
    expect(nothingToPropose(await loadConnectionProposal(created, sources()))).toBe(true);
  });

  test("an agent whose connections are all there is passed over", async () => {
    // GIVEN an agent whose two required connections exist on this machine
    const connections = [connection("mcp:web-browser", "mcp"), connection("mcp:com.notion/mcp", "mcp")];

    // WHEN the step reads what to propose
    const proposal = await loadConnectionProposal(AGENT, sources({ connections: async () => connections }));

    // THEN no line is left, and the step is passed over
    expect(proposal?.lines).toEqual([]);
    expect(nothingToPropose(proposal)).toBe(true);
  });

  test("a package that requires nothing is passed over", async () => {
    // GIVEN a package whose manifest requires no connection
    const none = sources({ manifest: async () => ({ package: { name: "request-sorter" } }) });

    // WHEN the step reads what to propose
    // THEN it is passed over
    expect(nothingToPropose(await loadConnectionProposal(AGENT, none))).toBe(true);
  });

  test("a package or a connection list that cannot be read claims nothing missing", async () => {
    // GIVEN a package that cannot be read, then connections that cannot be read
    const failing = async (): Promise<never> => {
      throw new Error("unreachable");
    };

    // WHEN the step reads what to propose in each case
    // THEN it proposes nothing rather than a connection it did not see missing
    expect(await loadConnectionProposal(AGENT, sources({ manifest: failing }))).toBeNull();
    expect(await loadConnectionProposal(AGENT, sources({ connections: failing }))).toBeNull();
  });

  test("an agent with a missing connection is not passed over", async () => {
    // GIVEN the agent whose connections are missing
    const proposal = await loadConnectionProposal(AGENT, sources());

    // WHEN the modal asks whether to pass the step over
    // THEN it does not, and the step after the first agent is this one
    expect(nothingToPropose(proposal)).toBe(false);
    expect(nextStep("first-agent")).toBe("connection");
  });

  test("the modal reads the proposal before the step opens, and steps over it when empty", () => {
    // GIVEN the source of the modal
    const modal = source("./OnboardingModal.svelte");

    // WHEN the move to the step is read
    // THEN the proposal is loaded for the agent kept, and an empty one passes the step
    expect(modal).toContain(
      "connectionProposal = await loadConnectionProposal(get(onboardingFirstAgent), connectionSources);",
    );
    expect(modal).toContain(
      'if (nextStep(step) === "connection" && (await connectionIsPassed())) passed.push("connection");',
    );
    expect(modal).toContain("const next = nextStep(step, passed);");
    // AND the step is mounted only with something to show
    expect(modal).toContain('{:else if currentStep === "connection" && connectionProposal}');
  });
});

describe("connection step - Skip is always offered", () => {
  const step = source("./OnboardingConnection.svelte");
  const footer = step.slice(step.indexOf('<footer class="setup-footer">'), step.indexOf("</footer>"));

  test("the footer carries Skip outside any condition", () => {
    // GIVEN the footer of the step
    // WHEN its controls are read
    // THEN Skip is there, moves on, and no state of the lines decides whether it shows
    expect(footer).toContain('data-testid="connection-skip"');
    expect(footer).toContain('<Button variant="outline" size="default" onclick={onnext} data-testid="connection-skip">');
    expect(footer).not.toContain("{#if");
    expect(footer).toContain('data-testid="connection-continue"');
    expect(footer).toContain('data-testid="connection-back"');
  });

  test("every sentence of the step exists in both languages", () => {
    // GIVEN every sentence the step may show
    const lines = missingLines(requiredConnections(MANIFEST), [], [BROWSER_ENTRY, NOTION_ENTRY]);
    const keys = new Set(["title", "add", "present", "add_later", "skip"]);
    for (const shown of [lines, lines.slice(0, 1)]) {
      keys.add(subtitleKey(shown).replace("onboarding.connection.", ""));
      keys.add(subtitleKey(refreshedLines(shown, [], new Set(shown.map((l) => l.type)))).replace("onboarding.connection.", ""));
    }
    expect(keys.size).toBe(9);

    // WHEN each is looked up in the two catalogues
    // THEN each is there, says something, and names the agent where it speaks of it
    for (const catalogue of [fr, en]) {
      const texts = catalogue.onboarding.connection as Record<string, string>;
      for (const key of keys) {
        expect(texts[key]?.trim(), key).toBeTruthy();
        if (key.startsWith("subtitle_")) expect(texts[key], key).toContain("{name}");
      }
    }
  });
});

describe("connection step - a connection added shows as present", () => {
  test("the line the add dialog added turns present, the other stays missing", () => {
    // GIVEN the two missing lines of the step
    const lines = missingLines(requiredConnections(MANIFEST), [], [BROWSER_ENTRY, NOTION_ENTRY]);

    // WHEN the add dialog reports the browser connection as added
    const after = refreshedLines(lines, [], new Set(["@modelcontextprotocol/server-puppeteer"]));

    // THEN that line is present, still listed, and nothing more can be added on it
    expect(after.map((line) => line.state)).toEqual(["present", "missing"]);
    expect(after).toHaveLength(2);
    expect(entryToAdd(after, "@modelcontextprotocol/server-puppeteer")).toBeNull();
    expect(allPresent(after)).toBe(false);
  });

  test("a connection found when the list is read again turns present too", () => {
    // GIVEN the two missing lines, and a list that now holds both connections
    const lines = missingLines(requiredConnections(MANIFEST), [], [BROWSER_ENTRY, NOTION_ENTRY]);
    const connections = [connection("mcp:web-browser", "mcp"), connection("mcp:com.notion/mcp", "mcp")];

    // WHEN the lines are refreshed
    const after = refreshedLines(lines, connections);

    // THEN both are present, and the sentence under the title says so
    expect(allPresent(after)).toBe(true);
    expect(subtitleKey(lines)).toBe("onboarding.connection.subtitle_missing_many");
    expect(subtitleKey(after)).toBe("onboarding.connection.subtitle_present_many");
  });

  test("a line once present stays present if the list cannot be read again", () => {
    // GIVEN a line the add dialog added
    const lines = missingLines(requiredConnections(MANIFEST), [], [BROWSER_ENTRY, NOTION_ENTRY]);
    const added = refreshedLines(lines, [], new Set(["com.notion/mcp"]));

    // WHEN the lines are refreshed against an empty list, with nothing reported
    const after = refreshedLines(added, []);

    // THEN the line is still present
    expect(after.map((line) => line.state)).toEqual(["missing", "present"]);
  });

  test("the step marks the line when the add dialog completes, and draws it as present", () => {
    // GIVEN the source of the step
    const step = source("./OnboardingConnection.svelte");

    // WHEN the wiring of the add dialog is read
    // THEN its completion marks the line added, and a present line shows it
    expect(step).toContain("if (adding !== null) void markAdded(adding);");
    expect(step).toContain("lines = refreshedLines(lines, [], added);");
    expect(step).toContain("lines = refreshedLines(lines, await connectionSources.connections(), added);");
    expect(step).toContain('{#if line.state === "present"}');
    expect(step).toContain('data-testid="connection-line-present"');
    expect(step).toContain('$t("onboarding.connection.present")');
  });
});
