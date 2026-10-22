/**
 * The decisions of the launch dialog of a batch: the sources it offers, the
 * accesses it grants, the failures it tolerates, what it refuses and the body
 * it sends, down to the command the launch reaches.
 */
import { beforeEach, describe, expect, it, vi } from "vitest";

const ipc = vi.hoisted(() => vi.fn(async (_cmd: string, _args?: unknown): Promise<unknown> => null));
vi.mock("@tauri-apps/api/core", () => ({ invoke: ipc }));

import { launchBatch, listBatchAccesses, listBatchSources } from "$lib/ipc/batches";
import type { BatchAccess, BatchSummary, RegistrySource } from "$lib/ipc/batches";
import type { Connection } from "$lib/types";
import {
  DEFAULT_THRESHOLD,
  NEVER_INTERRUPT,
  ORIGINAL_WRITE_GRANT,
  UNBOUNDED_FAILURES,
  batchFileOf,
  buildLaunch,
  canWriteOriginal,
  familyChoices,
  connectionChoices,
  fileChoice,
  folderChoice,
  lastTaskOf,
  maxFailuresOf,
  refusalOf,
  sourceChoices,
  splitAccesses,
  type LaunchForm,
} from "./launchBatch";

const REGISTRY: RegistrySource[] = [
  { id: "s-1", kind: "file", label: "Requests", reference: "/data/requests.xlsx" },
  { id: "s-2", kind: "file", label: "Notes", reference: "/data/notes.pdf" },
  { id: "s-3", kind: "folder", label: "Clients", reference: "conn-folder-1" },
  { id: "s-4", kind: "mail", label: "Support", reference: "imap:support" },
  { id: "s-5", kind: "mail", label: "Old box", reference: "legacy-7" },
  { id: "s-6", kind: "profile", label: "Profile", reference: null },
  { id: "s-7", kind: "memory", label: "Memory", reference: null },
];

const ACCESSES: BatchAccess[] = [
  { accessId: "send_email", permits: "Send an email", family: "send_external", grantable: true },
  { accessId: "file_write", permits: "Write a file", family: "write_or_delete", grantable: true },
  { accessId: "run_python", permits: "Run a script", family: "execute_code", grantable: false },
];

function form(over: Partial<LaunchForm> = {}): LaunchForm {
  return {
    agent: "triage",
    source: fileChoice("/data/requests.xlsx"),
    granted: [],
    threshold: DEFAULT_THRESHOLD,
    destination: "copy",
    columns: "Category, Urgency",
    unreadOnly: false,
    ...over,
  };
}

describe("the sources the dialog offers", () => {
  it("lists the files, folders and mailboxes of the registry, and leaves the profile and the memory out", () => {
    // GIVEN a registry holding every kind of source
    // WHEN the dialog builds its choices
    const choices = sourceChoices(REGISTRY, null);

    // THEN each file, folder and mailbox is offered under its family
    expect(familyChoices(choices, "table").map((c) => c.label)).toEqual(["Requests", "Notes"]);
    expect(familyChoices(choices, "folder").map((c) => c.label)).toEqual(["Clients"]);
    expect(familyChoices(choices, "mail").map((c) => c.label)).toEqual(["Support", "Old box"]);
    expect(choices).toHaveLength(5);
  });

  it("gives a reader to what a batch reads and a reason to what it does not", () => {
    // GIVEN the choices of that registry
    const byLabel = Object.fromEntries(sourceChoices(REGISTRY, null).map((c) => [c.label, c]));

    // THEN a workbook and a known mailbox are read
    expect(byLabel.Requests.reader).toEqual({ kind: "spreadsheet", location: "/data/requests.xlsx" });
    expect(byLabel.Support.reader).toEqual({ kind: "mail", location: "imap:support" });
    // AND the rest says why it is not
    expect(byLabel.Notes.blocked).toBe("file_not_table");
    expect(byLabel.Clients.blocked).toBe("folder_unread");
    expect(byLabel["Old box"].blocked).toBe("mailbox_unknown");
  });

  it("puts the file the dialog was opened on first, once", () => {
    // GIVEN a file joined to the chat that the registry also holds
    // WHEN the choices are built on it
    const choices = sourceChoices(REGISTRY, "/data/requests.xlsx");

    // THEN it comes first and is not listed twice
    expect(choices[0].key).toBe("path:/data/requests.xlsx");
    expect(choices.filter((c) => c.reader?.location === "/data/requests.xlsx")).toHaveLength(1);
  });

  it("reads a CSV file with the CSV reader, whatever the case of its extension", () => {
    // GIVEN a CSV file on a Windows path
    // WHEN it is chosen
    const choice = fileChoice("C:\\data\\Leads.CSV");

    // THEN its reader is the CSV one and its label its file name
    expect(choice.reader).toEqual({ kind: "csv", location: "C:\\data\\Leads.CSV" });
    expect(choice.label).toBe("Leads.CSV");
  });
});

function connection(over: Partial<Connection>): Connection {
  return {
    id: "c",
    kind: "folder",
    label: "c",
    account: null,
    provenance: "custom",
    remote: false,
    official: false,
    state: "connected",
    status: "working",
    reason: null,
    checkedAt: null,
    package: null,
    tools: [],
    ...over,
  };
}

describe("the folders and mailboxes of the connections", () => {
  it("offers a connected folder, an IMAP mailbox and a Microsoft account read as Outlook", () => {
    // GIVEN a folder, an IMAP mailbox, a Microsoft and a Google account, and
    // a mailbox that needs to sign in again
    const connections = [
      connection({ id: "folder:quotes", kind: "folder", label: "Quotes", path: "/data/quotes" }),
      connection({ id: "imap:ada@example.org", kind: "imap", label: "Mail", account: "ada@example.org" }),
      connection({ id: "native:microsoft:ada@example.com", kind: "native", label: "Microsoft", account: "ada@example.com" }),
      connection({ id: "native:google:ada@example.com", kind: "native", label: "Google", account: "ada@example.com" }),
      connection({ id: "imap:old@example.org", kind: "imap", account: "old@example.org", state: "needs_reauth" }),
    ];

    // WHEN the dialog builds its choices
    const choices = connectionChoices(connections);

    // THEN the folder is read by its path, the mailboxes by their account
    expect(choices.map((c) => c.reader)).toEqual([
      { kind: "folder", location: "/data/quotes" },
      { kind: "mail", location: "imap:ada@example.org" },
      { kind: "mail", location: "outlook:ada@example.com" },
    ]);
    // AND they reach the families of the dialog
    const offered = sourceChoices([], null, connections);
    expect(familyChoices(offered, "folder")).toHaveLength(1);
    expect(familyChoices(offered, "mail")).toHaveLength(2);
  });

  it("reads a folder picked on this computer, with its sub-folders when asked", () => {
    // GIVEN a folder picked on the spot
    const source = folderChoice("/data/quotes");

    // WHEN the batch is launched on it with its sub-folders
    const body = buildLaunch(form({ source, recursive: true }), []);

    // THEN the folder reader reads it, and nothing is written into it
    expect(body?.source).toEqual({ kind: "folder", location: "/data/quotes", options: { recursive: "true" } });
  });
});

describe("what the agent is asked", () => {
  it("sends the instruction, the fields of the answer as columns, and the synthesis", () => {
    // GIVEN a form with an instruction, two fields and a synthesis
    const asked = form({
      instruction: " Read the quote. ",
      outputs: [
        { name: "Supplier", description: "who sends it" },
        { name: " ", description: "an unnamed row is left out" },
        { name: "Total", description: "" },
      ],
      synthesis: "Rank them",
    });

    // WHEN the launch is built
    const body = buildLaunch(asked, []);

    // THEN the task holds them, and the columns written are the fields
    expect(body?.task).toEqual({
      instruction: "Read the quote.",
      outputs: [
        { name: "Supplier", description: "who sends it" },
        { name: "Total", description: "" },
      ],
      synthesis: "Rank them",
    });
    expect(body?.source.write?.columns).toEqual(["Supplier", "Total"]);
    // AND two fields of one name are refused
    expect(refusalOf(form({ outputs: [{ name: "Total", description: "" }, { name: "total", description: "" }] }))).toBe(
      "columns_duplicate",
    );
  });
});

describe("the file joined to the chat", () => {
  it("offers a batch on a dropped workbook or CSV file only", () => {
    // GIVEN files joined to the chat, with and without a path
    // THEN a table whose path is known can be processed row by row
    expect(batchFileOf({ name: "requests.xlsx", absolutePath: "/data/requests.xlsx" })).toBe("/data/requests.xlsx");
    expect(batchFileOf({ name: "leads.csv", absolutePath: "/data/leads.csv" })).toBe("/data/leads.csv");
    // AND a file picked without a path, or one that is no table, cannot
    expect(batchFileOf({ name: "requests.xlsx" })).toBeNull();
    expect(batchFileOf({ name: "report.pdf", absolutePath: "/data/report.pdf" })).toBeNull();
  });
});

describe("the sensitive accesses", () => {
  it("separates those a launch grants from those asked at every use", () => {
    // WHEN the accesses of the agent are split
    const split = splitAccesses(ACCESSES);

    // THEN executing code is never among the grantable ones
    expect(split.grantable.map((a) => a.accessId)).toEqual(["send_email", "file_write"]);
    expect(split.asked.map((a) => a.accessId)).toEqual(["run_python"]);
  });

  it("sends the accesses checked, and none the agent does not list as grantable", () => {
    // GIVEN a form where one access is checked, with one that cannot be granted and one unknown
    const checked = form({ granted: ["send_email", "run_python", "invented"] });

    // WHEN the launch is built
    const launch = buildLaunch(checked, ACCESSES);

    // THEN only the grantable access is granted
    expect(launch?.grants).toEqual(["send_email"]);
  });

  it("grants nothing when nothing is checked", () => {
    // WHEN a form with no access checked is launched
    const launch = buildLaunch(form(), ACCESSES);

    // THEN the batch holds no grant
    expect(launch?.grants).toEqual([]);
  });
});

describe("the failure threshold", () => {
  it("starts from ten failures", () => {
    // THEN the default choice tolerates ten failures
    expect(maxFailuresOf(DEFAULT_THRESHOLD)).toBe(10);
    expect(buildLaunch(form(), ACCESSES)?.maxFailures).toBe(10);
  });

  it("sends the count chosen", () => {
    // WHEN fifty failures are chosen
    const launch = buildLaunch(form({ threshold: "50" }), ACCESSES);

    // THEN the launch tolerates fifty
    expect(launch?.maxFailures).toBe(50);
  });

  it("never interrupts when asked not to", () => {
    // WHEN the choice is to never interrupt
    const launch = buildLaunch(form({ threshold: NEVER_INTERRUPT }), ACCESSES);

    // THEN the launch tolerates more failures than a batch can hold
    expect(launch?.maxFailures).toBe(UNBOUNDED_FAILURES);
  });

  it("falls back to the default on a choice that is no count", () => {
    // THEN an unreadable choice is the default one
    expect(maxFailuresOf("many")).toBe(10);
  });
});

describe("what a launch is refused for", () => {
  it("refuses a launch without a source", () => {
    // GIVEN a form whose source was never chosen
    const empty = form({ source: null });

    // THEN it is refused for that reason, and no body is built
    expect(refusalOf(empty)).toBe("source_missing");
    expect(buildLaunch(empty, ACCESSES)).toBeNull();
  });

  it("refuses a source no batch reads, and a launch without an agent", () => {
    // THEN a file that is no table, and a missing agent, are each refused
    expect(refusalOf(form({ source: fileChoice("/data/notes.pdf") }))).toBe("source_blocked");
    expect(refusalOf(form({ agent: null }))).toBe("agent_missing");
  });

  it("refuses result columns that are missing, repeated or too many", () => {
    // THEN each fault of the columns has its reason
    expect(refusalOf(form({ columns: " , " }))).toBe("columns_missing");
    expect(refusalOf(form({ columns: "Category, category" }))).toBe("columns_duplicate");
    const many = Array.from({ length: 33 }, (_, i) => `c${i}`).join(",");
    expect(refusalOf(form({ columns: many }))).toBe("columns_too_many");
  });

  it("asks a mailbox for no column", () => {
    // GIVEN a mailbox as the source, and no column typed
    const mailbox = sourceChoices(REGISTRY, null).find((c) => c.label === "Support") ?? null;

    // THEN the launch is accepted and writes nothing back
    const launch = buildLaunch(form({ source: mailbox, columns: "", unreadOnly: true }), ACCESSES);
    expect(launch?.source).toEqual({ kind: "mail", location: "imap:support", options: { unread_only: "true" } });
  });
});

describe("where the results go", () => {
  it("writes a copy by default, one column per field the agent returns", () => {
    // WHEN a workbook is launched with two columns
    const launch = buildLaunch(form(), ACCESSES);

    // THEN the results go into a copy, in those columns, without the grant of the original
    expect(launch?.source.write).toEqual({ destination: "copy", columns: ["Category", "Urgency"] });
    expect(launch?.grants).not.toContain(ORIGINAL_WRITE_GRANT);
  });

  it("writes the original only on request, and then grants that write", () => {
    // WHEN the original of an .xlsx workbook is asked for
    const launch = buildLaunch(form({ destination: "original" }), ACCESSES);

    // THEN the launch says so and carries the access that covers it
    expect(launch?.source.write?.destination).toBe("original");
    expect(launch?.grants).toEqual([ORIGINAL_WRITE_GRANT]);
  });

  it("keeps to a copy when the file cannot be modified in place", () => {
    // GIVEN a CSV file and an old workbook, with the original asked for
    for (const path of ["/data/leads.csv", "/data/old.xls"]) {
      const source = fileChoice(path);

      // THEN neither can be written in place, and the launch stays a copy
      expect(canWriteOriginal(source)).toBe(false);
      const launch = buildLaunch(form({ source, destination: "original" }), ACCESSES);
      expect(launch?.source.write?.destination).toBe("copy");
      expect(launch?.grants).toEqual([]);
    }
  });
});

describe("the commands behind the dialog", () => {
  beforeEach(() => {
    ipc.mockReset();
  });

  it("reads the sources and the accesses, and sends the launch whole", async () => {
    // GIVEN a shell that answers the three commands
    ipc.mockResolvedValueOnce(REGISTRY).mockResolvedValueOnce(ACCESSES).mockResolvedValueOnce({
      batchId: "b-1", agent: "triage", state: "running", total: 3, maxFailures: 10,
    });
    const launch = buildLaunch(form({ granted: ["send_email"] }), ACCESSES);
    if (launch === null) throw new Error("the launch was refused");

    // WHEN the dialog loads, then launches
    await listBatchSources();
    await listBatchAccesses("triage");
    const batch = await launchBatch(launch);

    // THEN each call reaches its command with its arguments
    expect(ipc.mock.calls).toEqual([
      ["list_batch_sources"],
      ["list_batch_accesses", { agent: "triage" }],
      ["launch_batch", { launch }],
    ]);
    expect(launch).toEqual({
      agent: "triage",
      source: {
        kind: "spreadsheet",
        location: "/data/requests.xlsx",
        options: {},
        write: { destination: "copy", columns: ["Category", "Urgency"] },
      },
      grants: ["send_email"],
      maxFailures: 10,
    });
    expect(batch.total).toBe(3);
  });

  it("lets the refusal of the runtime reach the dialog", async () => {
    // GIVEN a runtime that refuses the launch
    ipc.mockRejectedValueOnce("the source could not be read");
    const launch = buildLaunch(form(), ACCESSES);
    if (launch === null) throw new Error("the launch was refused");

    // THEN the call rejects with its reason
    await expect(launchBatch(launch)).rejects.toBe("the source could not be read");
  });
});

describe("the request a launch starts from", () => {
  it("takes the request of the latest batch of the agent that said what to do", () => {
    // GIVEN batches of two agents, one of them without a request
    const batch = (agent: string, createdAt: string, instruction: string): BatchSummary => ({
        batchId: createdAt,
        agent,
        source: { kind: "folder", location: "/data/Quotes", options: {}, task: { instruction, outputs: [] } },
        grants: [],
        concurrency: 1,
        maxFailures: 10,
        state: "completed",
        total: 1,
        completed: 1,
        failed: 0,
        canceled: 0,
        awaiting: 0,
        createdAt,
        updatedAt: createdAt,
        endedAt: createdAt,
      });
    const batches = [
      batch("audit", "2026-10-19 09:00:00", "Check the quote."),
      batch("audit", "2026-10-20 03:30:00", "Check the quote against the terms."),
      batch("audit", "2026-10-20 04:00:00", ""),
      batch("triage", "2026-10-20 05:00:00", "Sort the mail."),
    ];

    // WHEN the request of "audit" is looked for
    const latest = lastTaskOf([...batches], "audit");

    // THEN the latest one with an instruction is taken, and an agent with none has none
    expect(latest?.task.instruction).toBe("Check the quote against the terms.");
    expect(latest?.createdAt).toBe("2026-10-20 03:30:00");
    expect(lastTaskOf([...batches], "nobody")).toBeNull();
  });
});
