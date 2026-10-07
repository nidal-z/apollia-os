import { describe, expect, test } from "vitest";
import type { Source, SourceIndex } from "$lib/ipc/sources";
import type { Connection } from "$lib/types";
import {
  SOURCE_STATE_BADGE,
  addChoices,
  awaitsIndex,
  existingSource,
  fileName,
  folderCandidates,
  indexedSources,
  mailCandidates,
  readingToGrant,
  sourceOn,
  sourceOnFile,
  sourceRows,
  sourceStatusKey,
} from "./agentSources";

function source(over: Partial<Source>): Source {
  return {
    id: "src-1",
    kind: "folder",
    label: "Client requests",
    group: null,
    reference: "folder:4f1c",
    indexState: "absent",
    addedAt: "2026-10-01T08:00:00.000Z",
    ...over,
  };
}

function index(over: Partial<SourceIndex>): SourceIndex {
  return { id: "src-1", indexState: "ready", documents: 0, chunks: 0, skipped: [], ...over };
}

function connection(over: Partial<Connection>): Connection {
  return {
    id: "folder:4f1c",
    kind: "folder",
    label: "Client requests",
    account: null,
    path: "/Users/ada/Requests",
    provenance: "custom",
    remote: false,
    official: false,
    state: "connected",
    tools: [],
    ...over,
  };
}

const REGISTRY: Source[] = [
  source({ id: "src-mail", kind: "mail", label: "Support", reference: "imap:support@example.fr" }),
  source({ id: "src-code", kind: "code_folder", label: "Scripts", reference: "folder:9e2d" }),
  source({ id: "src-file", kind: "file", label: "Prices.pdf", reference: "/Users/ada/Prices.pdf" }),
  source({ id: "src-folder" }),
];

describe("agent sources - the list", () => {
  test("the adjustment lists every source of the registry kind by kind, then the profile and the memory", () => {
    // GIVEN a registry holding one source of each kind, and an agent consulting the folder and its memory
    // WHEN the adjustment lists the sources
    const rows = sourceRows(REGISTRY, {}, ["src-folder", "memory"], true);

    // THEN every source is offered, kind by kind, each checked or not
    expect(rows.map((row) => [row.kind, row.id, row.checked])).toEqual([
      ["folder", "src-folder", true],
      ["file", "src-file", false],
      ["mail", "src-mail", false],
      ["code_folder", "src-code", false],
      ["profile", "profile", false],
      ["memory", "memory", true],
    ]);
  });

  test("the reading lists only what the agent consults", () => {
    // GIVEN the same registry and an agent consulting the file and its profile
    // WHEN the sheet is read
    const rows = sourceRows(REGISTRY, {}, ["profile", "src-file"], false);

    // THEN only those two are listed, and only the file can be removed from the registry
    expect(rows.map((row) => row.id)).toEqual(["src-file", "profile"]);
    expect(rows.map((row) => row.registered)).toEqual([true, false]);
  });

  test("a source the registry no longer holds stays listed as missing, so it can be unchecked", () => {
    // GIVEN an agent naming a source that was removed
    // WHEN the adjustment lists the sources
    const rows = sourceRows([], {}, ["src-gone"], true);

    // THEN it comes last, checked and marked missing
    const last = rows[rows.length - 1];
    expect(last).toMatchObject({ id: "src-gone", kind: "missing", state: "missing", checked: true });
    expect(sourceStatusKey(last)).toBe("agent_sources.status_missing");
  });

  test("a profile or memory row of the registry is never listed twice", () => {
    // GIVEN a registry that holds a row of kind profile
    // WHEN the adjustment lists the sources
    const rows = sourceRows([source({ id: "src-p", kind: "profile", reference: null })], {}, [], true);

    // THEN the profile appears once, by its token
    expect(rows.map((row) => row.id)).toEqual(["profile", "memory"]);
  });
});

describe("agent sources - the state of the index", () => {
  test("each state of the index has its line and its badge", () => {
    // GIVEN one folder source in each state of its index
    const states = ["absent", "indexing", "ready", "error", "keywords_only"] as const;

    // WHEN its row is built from the index the runtime answers
    const rows = states.map(
      (state) => sourceRows([source({})], { "src-1": index({ indexState: state, documents: 12 }) }, ["src-1"], false)[0],
    );

    // THEN the line and the badge follow the state, and the files read are carried
    expect(rows.map((row) => row.state)).toEqual(["pending", "indexing", "ready", "error", "keywords_only"]);
    expect(rows.map(sourceStatusKey)).toEqual([
      "agent_sources.status_pending",
      "agent_sources.status_indexing",
      "agent_sources.status_ready",
      "agent_sources.status_error",
      "agent_sources.status_keywords_only",
    ]);
    expect(rows.map((row) => SOURCE_STATE_BADGE[row.state]?.tone)).toEqual([
      "neutral",
      "info",
      "success",
      "danger",
      "warning",
    ]);
    expect(rows[1].documents).toBe(12);
  });

  test("the files left out are counted by reason and listed", () => {
    // GIVEN an index that left out two files over 50 MB and one unreadable
    const skipped = [
      { path: "/r/video.mov", sizeBytes: 60_000_000, reason: "too_large" as const },
      { path: "/r/scan.pdf", sizeBytes: 900, reason: "unreadable" as const },
      { path: "/r/dump.csv", sizeBytes: 70_000_000, reason: "too_large" as const },
    ];

    // WHEN the row is built
    const [row] = sourceRows([source({})], { "src-1": index({ documents: 214, skipped }) }, ["src-1"], false);

    // THEN both counts and the paths are carried
    expect([row.documents, row.tooLarge, row.unreadable]).toEqual([214, 2, 1]);
    expect(row.skipped.map((file) => file.path)).toEqual(["/r/video.mov", "/r/scan.pdf", "/r/dump.csv"]);
  });

  test("without an index answer the row shows the state the registry records", () => {
    // GIVEN a source the registry records as keywords only, and no index answer
    // WHEN the row is built
    const [row] = sourceRows([source({ indexState: "keywords_only" })], {}, ["src-1"], false);

    // THEN the row says keywords only
    expect(row.state).toBe("keywords_only");
  });

  test("the section asks again while a source that points at documents is not read yet", () => {
    // GIVEN rows being read, rows ready, and a mailbox nothing indexes
    const reading = sourceRows([source({ indexState: "indexing" })], {}, ["src-1"], false);
    const ready = sourceRows([source({ indexState: "ready" })], {}, ["src-1", "profile"], false);
    const mail = sourceRows([REGISTRY[0]], {}, ["src-mail"], false);

    // WHEN asked whether to read the indexes again
    // THEN only the rows being read ask for it
    expect(awaitsIndex(reading)).toBe(true);
    expect(awaitsIndex(ready)).toBe(false);
    expect(awaitsIndex(mail)).toBe(false);
  });

  test("an index is read for the sources that point at documents only", () => {
    // GIVEN one source of each kind
    // WHEN the sources whose index is read are picked
    // THEN the mailbox is left out
    expect(indexedSources(REGISTRY).map((s) => s.id)).toEqual(["src-code", "src-file", "src-folder"]);
  });
});

describe("agent sources - adding", () => {
  test("the menu offers a folder, a file, a mailbox, the profile and the memory, and a code folder in developer mode only", () => {
    // GIVEN the kinds the runtime offers on a folder, developer mode off then on
    // WHEN the menu is built
    // THEN the code folder is offered only when the runtime offers it
    expect(addChoices(["folder"])).toEqual(["folder", "file", "mail", "profile", "memory"]);
    expect(addChoices(["folder", "code_folder"])).toEqual(["folder", "file", "mail", "profile", "memory", "code_folder"]);
  });

  test("a folder is chosen among the folder connections", () => {
    // GIVEN a folder connection, a mailbox and an MCP server
    const connections = [
      connection({}),
      connection({ id: "imap:ana@example.fr", kind: "imap", label: "Ana", account: "ana@example.fr", path: undefined }),
      connection({ id: "mcp:notion", kind: "mcp", label: "Notion", path: undefined }),
    ];

    // WHEN the candidates of a folder source are listed, and one is chosen
    const candidates = folderCandidates(connections);
    const added = sourceOn("folder", candidates[0]);

    // THEN only the folder is offered, and the source points at the connection
    expect(candidates).toEqual([{ reference: "folder:4f1c", label: "Client requests", detail: "/Users/ada/Requests" }]);
    expect(added).toEqual({ kind: "folder", label: "Client requests", reference: "folder:4f1c" });
  });

  test("a mailbox is chosen among the IMAP mailboxes and the Outlook accounts", () => {
    // GIVEN an IMAP mailbox, a Microsoft account, a Google account and a folder
    const connections = [
      connection({ id: "imap:support@example.fr", kind: "imap", label: "Support", account: "support@example.fr" }),
      connection({ id: "native:microsoft:ana@example.fr", kind: "native", label: "Microsoft", account: "ana@example.fr" }),
      connection({ id: "native:google:ana@gmail.com", kind: "native", label: "Google", account: "ana@gmail.com" }),
      connection({}),
    ];

    // WHEN the candidates of a mail source are listed
    const candidates = mailCandidates(connections);

    // THEN each names the place its mailbox is read from
    expect(candidates.map((c) => c.reference)).toEqual(["imap:support@example.fr", "outlook:ana@example.fr"]);
    expect(sourceOn("mail", candidates[0])).toEqual({
      kind: "mail",
      label: "Support",
      reference: "imap:support@example.fr",
    });
  });

  test("a file is added by its path and named by its file name", () => {
    // GIVEN a path the system picker answered, on either kind of machine
    // WHEN the source is built
    // THEN it points at the path and carries the file name
    expect(sourceOnFile("/Users/ada/Docs/Prices 2026.pdf")).toEqual({
      kind: "file",
      label: "Prices 2026.pdf",
      reference: "/Users/ada/Docs/Prices 2026.pdf",
    });
    expect(fileName("C:\\Docs\\Prices.pdf")).toBe("Prices.pdf");
  });

  test("a source the registry already holds is checked, not added twice", () => {
    // GIVEN a registry holding a folder source on a connection
    // WHEN the same folder is added again, then as a code folder
    // THEN the first is found, the second is a new source
    const wanted = { kind: "folder" as const, label: "Client requests", reference: "folder:4f1c" };
    expect(existingSource(REGISTRY, wanted)?.id).toBe("src-folder");
    expect(existingSource(REGISTRY, { ...wanted, kind: "code_folder" })).toBeNull();
  });
});

describe("agent sources - a source grants no tool", () => {
  const tools = [
    { id: "mcp:fs/read_file", family: "read" as const, familySource: "server" as const, prechecked: true },
    { id: "mcp:fs/list", family: "read" as const, familySource: "server" as const, prechecked: true },
    { id: "mcp:fs/write_file", family: "write_or_delete" as const, familySource: "server" as const, prechecked: false },
  ];
  const connections = [connection({ tools })];

  test("a checked folder the agent cannot read offers the reading tools of its connection, and nothing else", () => {
    // GIVEN a checked folder source and an agent granted none of the tools of its connection
    const [row] = sourceRows([source({})], {}, ["src-1"], true);

    // WHEN the note of the row is computed
    const offered = readingToGrant(row, connections, ["file_read"]);

    // THEN the reading tools are offered, never the writing one
    expect(offered).toEqual(["mcp:fs/read_file", "mcp:fs/list"]);
  });

  test("nothing is offered once the agent reads the folder, for an unchecked source, or for another kind", () => {
    // GIVEN the same folder, read by the agent, then unchecked, then a file source
    const [checked] = sourceRows([source({})], {}, ["src-1"], true);
    const [unchecked] = sourceRows([source({})], {}, [], true);
    const [file] = sourceRows([REGISTRY[2]], {}, ["src-file"], true);

    // WHEN the note of each row is computed
    // THEN none offers anything
    expect(readingToGrant(checked, connections, ["mcp:fs/list"])).toEqual([]);
    expect(readingToGrant(unchecked, connections, [])).toEqual([]);
    expect(readingToGrant(file, connections, [])).toEqual([]);
  });
});
