import { describe, expect, test } from "vitest";
import type { AgentChangeEntry } from "$lib/ipc/agentSheet";
import {
  ACCESS_GROUP_LABEL,
  ACCESS_GROUP_RULE,
  CHANGE_LABEL,
  PROVENANCE_LABEL,
  SHEET_SECTIONS,
  accessView,
  changeViews,
  fallbackNotice,
  groupAccesses,
  memoryRowsFromEntries,
  memoryRowsFromSearch,
  missionView,
  modelView,
  readableName,
  readsProfile,
  sourceViews,
  withoutRow,
} from "./agentSheet";

describe("agent sheet - order", () => {
  test("the sections read mission, model, access, sources, activity, memory", () => {
    // GIVEN the sections of the sheet
    // WHEN they are listed
    const order = [...SHEET_SECTIONS];

    // THEN they come in the order the sheet is read
    expect(order).toEqual(["mission", "model", "access", "sources", "activity", "memory"]);
  });
});

describe("agent sheet - mission", () => {
  test("the resolved mission and examples win over what the code declares", () => {
    // GIVEN a resolved agent with a mission and examples, and a declared description
    const resolved = { mission: "Sort the invoices", examples: ["Sort this week's invoices"] };
    const declared = { description: "Old description", examples: ["Old example"] };

    // WHEN the mission section is built
    const view = missionView(resolved, declared);

    // THEN it shows the resolved mission and examples
    expect(view).toEqual({ mission: "Sort the invoices", examples: ["Sort this week's invoices"] });
  });

  test("an agent without definition reads its declared description", () => {
    // GIVEN no resolved agent and a declared description with examples
    // WHEN the mission section is built
    const view = missionView(null, { description: "Summarise mails", examples: ["Summarise today"] });

    // THEN the declared description and examples show
    expect(view).toEqual({ mission: "Summarise mails", examples: ["Summarise today"] });
  });

  test("a blank mission reads as no mission", () => {
    // GIVEN a resolved agent whose mission is blank and nothing declared
    // WHEN the mission section is built
    const view = missionView({ mission: "   ", examples: [] }, null);

    // THEN the section says there is no mission
    expect(view.mission).toBeNull();
  });
});

describe("agent sheet - model", () => {
  test("a local assignment runs on this computer", () => {
    // GIVEN a local model assigned to the agent
    const assignment = { agent: "mail", model_source: "local" as const, model_id: "qwen3-8b", preset_id: null };

    // WHEN the model section is built
    const view = modelView(assignment, "default-model");

    // THEN it names the assigned model, local, and not the default
    expect(view).toEqual({ name: "qwen3-8b", place: "local", server: null, preset: null });
  });

  test("a server assignment names the server and the model", () => {
    // GIVEN a model on a declared server
    const assignment = { agent: "mail", model_source: "server" as const, model_id: "lab/mistral-small", preset_id: 3 };

    // WHEN the model section is built
    const view = modelView(assignment, null);

    // THEN the server and the model are split
    expect(view).toEqual({ name: "mistral-small", place: "server", server: "lab", preset: 3 });
  });

  test("without assignment the agent follows the default model", () => {
    // GIVEN no assignment and a default model
    // WHEN the model section is built
    const view = modelView(null, "qwen3-8b");

    // THEN it says it follows the default
    expect(view).toEqual({ name: "qwen3-8b", place: "default", server: null, preset: null });
  });

  test("the fallback notice names the model asked for and the one used", () => {
    // GIVEN a fallback recorded for the agent, and none for another
    const fallback = { agent: "mail", requested: "qwen3-8b", used: "llama-3", at: "2026-10-03T09:12:00Z" };

    // WHEN the notices are built
    const shown = fallbackNotice(fallback);
    const none = fallbackNotice(null);

    // THEN the first says which model served, the second shows nothing
    expect(shown).toEqual({ requested: "qwen3-8b", used: "llama-3", at: "2026-10-03T09:12:00Z" });
    expect(none).toBeNull();
  });
});

describe("agent sheet - access", () => {
  const native = (family: "read" | "send_external" | "execute_code" | "unclassified") => ({
    description: `Does ${family}`,
    family,
    family_source: "builtin" as const,
    kind: "native",
  });

  test("accesses are grouped by family whatever their provenance, every family listed", () => {
    // GIVEN a native read tool, a connection tool that sends, an agent and a workflow
    const accesses = [
      accessView("file_read", native("read")),
      accessView("mail_send", { description: "Sends a mail", family: "send_external", family_source: "server", kind: "mcp_server" }),
      accessView("agent:veille", null),
      accessView("workflow:weekly", null),
    ];

    // WHEN they are grouped
    const groups = groupAccesses(accesses);

    // THEN the five families and the agents group show, in order, empty ones included
    expect(groups.map((g) => g.key)).toEqual([
      "read",
      "read_personal",
      "write_or_delete",
      "send_external",
      "execute_code",
      "agents",
    ]);
    expect(groups.find((g) => g.key === "read")?.accesses.map((a) => a.id)).toEqual(["file_read"]);
    expect(groups.find((g) => g.key === "send_external")?.accesses.map((a) => a.id)).toEqual(["mail_send"]);
    expect(groups.find((g) => g.key === "agents")?.accesses.map((a) => a.id)).toEqual([
      "agent:veille",
      "workflow:weekly",
    ]);
    expect(groups.find((g) => g.key === "execute_code")?.accesses).toEqual([]);
  });

  test("an access says what it permits and where it comes from", () => {
    // GIVEN a connection tool and a package tool
    const connection = accessView("mail_read", {
      description: "Reads the mails of the account",
      family: "read_personal",
      family_source: "selection",
      kind: "mcp_server",
    });
    const packaged = accessView("classify", {
      description: "Classifies a request",
      family: "read",
      family_source: "package",
      kind: "python",
    });

    // WHEN their views are read
    // THEN each carries what it permits, its provenance and its sensitivity
    expect(connection).toMatchObject({ permits: "Reads the mails of the account", provenance: "connection", sensitive: true });
    expect(packaged).toMatchObject({ permits: "Classifies a request", provenance: "package", sensitive: false });
    expect(accessView("file_read", native("read")).provenance).toBe("builtin");
  });

  test("an access nobody describes is unclassified, sensitive, and named without its raw identifier", () => {
    // GIVEN an access the runtime does not describe
    // WHEN its view is built and grouped
    const view = accessView("legacy_tool.run", null);
    const groups = groupAccesses([view]);

    // THEN it shows in the unclassified group, sensitive, with a readable name
    expect(view).toMatchObject({ group: "unclassified", sensitive: true, permits: null, label: "Legacy tool run" });
    expect(groups.at(-1)?.key).toBe("unclassified");
  });

  test("the unclassified group only shows when it holds an access", () => {
    // GIVEN no access at all
    // WHEN the groups are built
    const groups = groupAccesses([]);

    // THEN unclassified is absent and the six other groups are there, empty
    expect(groups.map((g) => g.key)).not.toContain("unclassified");
    expect(groups).toHaveLength(6);
  });

  test("every group, provenance and change has its catalogue key spelled out", () => {
    // GIVEN the key tables of the sheet
    // WHEN their values are read
    const keys = [
      ...Object.values(ACCESS_GROUP_LABEL),
      ...Object.values(ACCESS_GROUP_RULE),
      ...Object.values(PROVENANCE_LABEL),
      ...Object.values(CHANGE_LABEL),
    ];

    // THEN every one is a key of the sheet's namespace
    expect(keys.every((k) => k.startsWith("agent_sheet."))).toBe(true);
  });

  test("a readable name drops prefixes and separators", () => {
    // GIVEN raw identifiers
    // WHEN they are made readable
    // THEN the prefix and the separators go
    expect(readableName("file_read")).toBe("File read");
    expect(readableName("mcp:outlook")).toBe("Outlook");
  });
});

describe("agent sheet - sources", () => {
  test("the profile and the memory are named, other sources show as written", () => {
    // GIVEN the sources checked for an agent
    // WHEN their views are built
    const views = sourceViews(["profile", "memory", "~/Documents/Devis"]);

    // THEN profile and memory are recognised
    expect(views.map((v) => v.kind)).toEqual(["profile", "memory", "other"]);
  });

  test("the agent reads the profile only when it is a checked source", () => {
    // GIVEN two lists of sources
    // WHEN the profile question is asked
    // THEN only the list holding the profile answers yes
    expect(readsProfile(["profile"])).toBe(true);
    expect(readsProfile(["memory"])).toBe(false);
  });
});

describe("agent sheet - activity", () => {
  const entry = (seq: number, kind: string, payload: Record<string, unknown>): AgentChangeEntry => ({
    seq,
    run_id: "governance:invoices",
    ts: `2026-10-0${seq + 1}T09:00:00Z`,
    kind,
    payload,
  });

  test("the changes come newest first, access changes included", () => {
    // GIVEN the governance history of an agent, oldest first
    const history = [
      entry(0, "agent_installed", { event: "installed", version: "1.0", actor: "user" }),
      entry(1, "access_changed", { event: "access_changed", access_id: "mail_send", change: "granted", actor: "api" }),
      entry(2, "access_changed", { event: "access_changed", access_id: "file_read", change: "revoked", actor: "api" }),
      entry(3, "agent_override_changed", { event: "override_changed", field: "mission", change: "set", actor: "api" }),
      entry(4, "agent_updated", { event: "updated", from_version: "1.0", to_version: "1.2", actor: "user" }),
    ];

    // WHEN the changes are built
    const changes = changeViews(history);

    // THEN they read newest first, each with its kind and subject
    expect(changes.map((c) => c.kind)).toEqual([
      "updated",
      "override_set",
      "access_revoked",
      "access_granted",
      "installed",
    ]);
    expect(changes[0].versions).toEqual(["1.0", "1.2"]);
    expect(changes[1].subject).toBe("mission");
    expect(changes[2].subject).toBe("file_read");
    expect(changes[3].actor).toBe("api");
  });

  test("a removed override and an unknown entry still read", () => {
    // GIVEN an override removed and an entry the sheet does not know
    // WHEN their views are built
    const changes = changeViews([
      entry(0, "agent_override_changed", { field: "sources", change: "removed" }),
      entry(1, "something_else", {}),
    ]);

    // THEN the first reads as back to default, the second as a recorded change
    expect(changes.map((c) => c.kind)).toEqual(["other", "override_removed"]);
  });
});

describe("agent sheet - memory", () => {
  const entries = [
    { id: "a", entry_type: "semantic" as const, key: "pref.tone", value: "Prefers a short answer", created_at: "2026-10-01T09:00:00Z", expires_at: null, score: null },
    { id: "b", entry_type: "episodic" as const, key: "quote.42", value: "", created_at: "2026-10-02T09:00:00Z", expires_at: null, score: null },
  ];

  test("the listing reads newest first, the key kept for developer mode", () => {
    // GIVEN two retained entries, one without value
    // WHEN the rows are built
    const rows = memoryRowsFromEntries(entries);

    // THEN the newest comes first and an entry without value shows its key
    expect(rows.map((r) => r.id)).toEqual(["b", "a"]);
    expect(rows[0].text).toBe("quote.42");
    expect(rows[1]).toMatchObject({ text: "Prefers a short answer", key: "pref.tone" });
  });

  test("a search keeps the ranking of the search", () => {
    // GIVEN two search results, best first
    const results = [
      { id: "x", entry_type: "semantic" as const, content: "Quote for Acme", score: 2, relevance: null, created_at: "2026-09-01T09:00:00Z" },
      { id: "y", entry_type: "episodic" as const, content: "Quote sent", score: 1, relevance: null, created_at: "2026-10-01T09:00:00Z" },
    ];

    // WHEN the rows are built
    const rows = memoryRowsFromSearch(results);

    // THEN the order of the search is kept
    expect(rows.map((r) => r.id)).toEqual(["x", "y"]);
  });

  test("deleting one entry leaves the others", () => {
    // GIVEN the rows of a listing
    const rows = memoryRowsFromEntries(entries);

    // WHEN one entry is deleted
    const left = withoutRow(rows, "a");

    // THEN only the other remains
    expect(left.map((r) => r.id)).toEqual(["b"]);
  });
});
