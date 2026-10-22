import { describe, test, expect } from "vitest";
import { readFileSync } from "node:fs";

import {
  ADD_GROUPS,
  ADD_WINDOW_CLOSED,
  addWindowGroups,
  backToSearch,
  chooseEntry,
  entryNeeds,
  findEntry,
  flowCancelled,
  flowDone,
  flowResumed,
  leaveForFlow,
  openAddWindow,
  openOnMailbox,
  startAdding,
  type AddEntry,
} from "$lib/connections/addWindow";
import type { RegistryServerView } from "$lib/types";

function entry(over: Partial<RegistryServerView> = {}): RegistryServerView {
  return {
    name: "com.notion/mcp",
    title: "Notion",
    description: "Pages and databases",
    version: "1.0.0",
    repository_url: null,
    website_url: null,
    packages: null,
    trust_level: "verified_official",
    category: "productivity",
    enrichment: null,
    is_installed: false,
    remotes: [],
    ...over,
  };
}

const WORDS: Record<string, string[]> = {
  "native:google": ["Google", "Gmail, Calendar, Drive"],
  "native:microsoft": ["Microsoft", "Outlook, OneDrive"],
  folder: ["Folder", "A folder of your computer"],
  mailbox: ["Mailbox", "Any mailbox with an app password"],
};

function groups(query: string, publicEntries: RegistryServerView[] = []) {
  return addWindowGroups({
    selection: [entry()],
    publicEntries,
    query,
    wordsOf: (key) => WORDS[key] ?? [],
  });
}

function keys(query: string, publicEntries: RegistryServerView[] = []): Record<string, string[]> {
  return Object.fromEntries(groups(query, publicEntries).map((g) => [g.key, g.entries.map((e) => e.key)]));
}

describe("one search over four groups", () => {
  test("an empty search shows every group in order", () => {
    // GIVEN the selection, nothing public read
    // WHEN nothing is typed
    const shown = groups("");

    // THEN the four groups come in order, each with its entries
    expect(shown.map((g) => g.key)).toEqual(ADD_GROUPS);
    expect(keys("")).toEqual({
      apollia: ["native:google", "native:microsoft", "entry:com.notion/mcp"],
      computer: ["folder", "mailbox"],
      public: [],
      address: ["address"],
    });
  });

  test("the words narrow every group but the address, which always remains", () => {
    // GIVEN the selection and one public entry
    const weather = entry({ name: "io.github.someone/weather", title: "Weather", description: "Forecasts" });

    // WHEN a word of the mailbox, then a word of the public entry, then nothing known, is typed
    const mail = keys("app password", [weather]);
    const forecast = keys("forecasts", [weather]);
    const none = keys("spreadsheet", [weather]);

    // THEN each search keeps what holds its words, and the address stays
    expect(mail).toEqual({ apollia: [], computer: ["mailbox"], public: [], address: ["address"] });
    expect(forecast.public).toEqual(["entry:io.github.someone/weather"]);
    expect(forecast.apollia).toEqual([]);
    expect(none).toEqual({ apollia: [], computer: [], public: [], address: ["address"] });
  });

  test("a chosen entry the search left out has no card", () => {
    // GIVEN the folder chosen
    // WHEN the search no longer holds it
    const found = findEntry(groups("notion"), "folder");

    // THEN no card is shown for it
    expect(found).toBeNull();
    expect(findEntry(groups("notion"), "entry:com.notion/mcp")?.kind).toBe("catalogue");
  });
});

describe("what an entry asks", () => {
  test("a selection entry says it needs a key, a download, or the network", () => {
    // GIVEN a package asking a secret, a remote service, and a package asking nothing
    const withKey = entry({
      packages: [
        {
          registryType: "npm",
          identifier: "@acme/server",
          version: null,
          runtimeHint: null,
          transport: { type: "stdio" },
          environmentVariables: [{ name: "ACME_TOKEN", description: null, isRequired: true, isSecret: true }],
          packageArguments: [],
        },
      ],
    });
    const remote = entry({ remotes: [{ type: "streamable-http", url: "https://mcp.notion.com/mcp", headers: [] }] });
    const plain = entry();
    const card = (e: RegistryServerView): AddEntry => ({ key: `entry:${e.name}`, kind: "catalogue", entry: e, source: "selection" });

    // WHEN their needs are listed
    // THEN each says what it will ask
    expect(entryNeeds(card(withKey))).toEqual(["key", "download"]);
    expect(entryNeeds(card(remote))).toEqual(["online"]);
    expect(entryNeeds(card(plain))).toEqual(["nothing"]);
    expect(entryNeeds({ key: "native:google", kind: "native", provider: "google" })).toEqual(["sign_in", "online"]);
    expect(entryNeeds({ key: "folder", kind: "folder" })).toEqual(["folder"]);
  });
});

describe("Cancel gives the search back, never the list", () => {
  test("cancel on a card keeps the words typed", () => {
    // GIVEN the window open on a search, a card open
    const searched = { ...openAddWindow(), query: "notion", chosen: "entry:com.notion/mcp" };

    // WHEN Cancel is pressed on the card
    const back = backToSearch(searched);

    // THEN the search stays, the card closes
    expect(back).toEqual({ open: true, query: "notion", chosen: null, adding: false, resume: null, away: false, mailbox: "" });
  });

  test("cancel in a flow not yet added keeps the words typed and closes the flow", () => {
    // GIVEN Add pressed on a card of a search
    const adding = startAdding({ ...openAddWindow(), query: "notion", chosen: "entry:com.notion/mcp" });

    // WHEN Cancel is pressed in its flow
    const back = backToSearch(adding);

    // THEN the search stays, neither the card nor the flow is open
    expect(adding.adding).toBe(true);
    expect(back).toEqual({ open: true, query: "notion", chosen: null, adding: false, resume: null, away: false, mailbox: "" });
  });

  test("cancelling the sign-in the flow opened reopens the window on its search", () => {
    // GIVEN the flow of an account that left the window for its sign-in dialog
    const searched = startAdding({ ...openAddWindow(), query: "outlook", chosen: "native:microsoft" });
    const adding = leaveForFlow(searched);

    // WHEN the sign-in is cancelled
    const cancelled = flowCancelled(adding);

    // THEN the window is open again on the same words, no card open
    expect(adding.open).toBe(false);
    expect(cancelled).toEqual({ open: true, query: "outlook", chosen: null, adding: false, resume: null, away: false, mailbox: "" });
  });

  test("an addition finished, or one the window did not open, does not reopen it", () => {
    // GIVEN an addition finished, and one opened from another screen
    const finished = flowCancelled(flowDone());
    const fromElsewhere = flowCancelled(leaveForFlow(ADD_WINDOW_CLOSED));

    // WHEN their dialogs close
    // THEN the window stays closed
    expect(finished.open).toBe(false);
    expect(fromElsewhere.open).toBe(false);
  });

  test("choosing another entry while a flow is open opens its card, not its flow", () => {
    // GIVEN the flow of one entry open
    const adding = startAdding({ ...openAddWindow(), chosen: "entry:com.notion/mcp" });

    // WHEN another entry is chosen in the list
    const chosen = chooseEntry(adding, "entry:com.linear/mcp");

    // THEN its card opens, its warning to read before anything is added
    expect(chosen).toEqual({ ...adding, chosen: "entry:com.linear/mcp", adding: false, resume: null });
  });

  test("the repair of a mailbox opens its flow, its address filled in, and leaving it forgets the address", () => {
    // GIVEN a mailbox whose password was refused
    // WHEN its repair opens the window
    const repair = openOnMailbox("ada@example.com");

    // THEN the window is open on the flow of a mailbox, its address given
    expect(repair).toEqual({ ...openAddWindow(), chosen: "mailbox", adding: true, mailbox: "ada@example.com" });
    // AND going back to the search, or to another entry, forgets it
    expect(backToSearch(repair).mailbox).toBe("");
    expect(chooseEntry(repair, "folder").mailbox).toBe("");
  });

  test("a sign-in done resumes the flow on who may use the account", () => {
    // GIVEN the flow of an account that left the window for its sign-in dialog
    const away = leaveForFlow(startAdding({ ...openAddWindow(), query: "outlook", chosen: "native:microsoft" }));

    // WHEN the dialog connects the account
    const resumed = flowResumed(away, "native:microsoft:ada@example.com");

    // THEN the window is back on the same card, its flow on that connection
    expect(resumed).toEqual({
      open: true,
      query: "outlook",
      chosen: "native:microsoft",
      adding: true,
      resume: "native:microsoft:ada@example.com",
      away: false,
      mailbox: "",
    });
    // AND a sign-in the window did not open resumes nothing
    expect(flowResumed(ADD_WINDOW_CLOSED, "native:microsoft:ada@example.com")).toEqual(ADD_WINDOW_CLOSED);
  });

  test("the Connections screen sends a cancelled sign-in back to the window, and a done one to the flow", () => {
    // GIVEN the source of the Connections screen
    const route = readFileSync(new URL("../../routes/Connections.svelte", import.meta.url), "utf8");

    // WHEN the close and connected handlers of the sign-in dialog are read
    const dialog = route.slice(route.indexOf("<NativeOauthDialog"), route.indexOf("/>", route.indexOf("<NativeOauthDialog")));

    // THEN a cancel gives the window back on its search, a sign-in done resumes its flow
    expect(dialog).toContain("addWindow = flowCancelled(addWindow)");
    expect(dialog).toContain("addWindow = flowResumed(addWindow, nativeConnectionId(provider, accountId))");
    // AND no separate dialog of a kind remains
    for (const gone of ["<ConnectorWizard", "<MailboxDialog", "<FolderConnectionDialog"]) {
      expect(route).not.toContain(gone);
    }
  });
});
