import { describe, it, expect } from "vitest";
import {
  connectionRows,
  isSelected,
  kindCounts,
  nativeProviderOf,
  needsAction,
  provenanceLine,
  rowMatches,
  serverNameOf,
  visibleRows,
} from "./connectionRows";
import type { Connection } from "$lib/types";

function connection(over: Partial<Connection> = {}): Connection {
  return {
    id: "mcp:files",
    kind: "mcp",
    label: "files",
    account: null,
    provenance: "selection",
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

const google = (account: string, over: Partial<Connection> = {}): Connection =>
  connection({
    id: `native:google:${account}`,
    kind: "native",
    label: "Google",
    account,
    remote: true,
    ...over,
  });

const repair = { status: "needs_repair", reason: { key: "unreachable" } } as const;

describe("connectionRows", () => {
  it("puts accounts, mailboxes, services and folders in one list, one row per connection", () => {
    // GIVEN two Google accounts, a mailbox, a server and a folder
    const connections = [
      connection({ id: "folder:invoices", kind: "folder", label: "Invoices" }),
      connection(),
      connection({ id: "imap:ada@example.com", kind: "imap", label: "Fastmail", account: "ada@example.com" }),
      google("ada@example.com"),
      google("bob@example.com"),
    ];

    // WHEN the rows are built
    const rows = connectionRows(connections);

    // THEN each connection has its own row, keyed by its identifier, each
    // Google account apart, by name then account
    expect(rows.map((r) => r.key)).toEqual([
      "imap:ada@example.com",
      "mcp:files",
      "native:google:ada@example.com",
      "native:google:bob@example.com",
      "folder:invoices",
    ]);
  });

  it("puts what asks for an action first, by name, then what works, by name", () => {
    // GIVEN two working servers, a paused one, a blocked one, one never checked and one to repair
    const connections = [
      connection({ id: "mcp:alpha", label: "alpha" }),
      connection({ id: "mcp:zeta", label: "zeta", ...repair }),
      connection({ id: "mcp:beta", label: "beta", status: "paused" }),
      connection({ id: "mcp:gamma", label: "gamma", status: "blocked" }),
      connection({ id: "mcp:omega", label: "omega" }),
      connection({ id: "mcp:delta", label: "delta", status: "not_checked", reason: { key: "not_checked" } }),
    ];

    // WHEN the rows are built
    const rows = connectionRows(connections);

    // THEN to repair, to check, paused and blocked lead by name, and the two
    // that work follow by name
    expect(rows.map((r) => r.label)).toEqual(["beta", "delta", "gamma", "zeta", "alpha", "omega"]);
  });

  it("asks an action of every state but working", () => {
    // GIVEN the five states a connection can be found in
    // WHEN each is asked whether it needs an action
    // THEN only working answers no
    expect(needsAction("working")).toBe(false);
    for (const status of ["needs_repair", "not_checked", "paused", "blocked"] as const) {
      expect(needsAction(status)).toBe(true);
    }
  });

  it("carries the state, its reason, its date and the package the runtime gave", () => {
    // GIVEN a server brought by a package, to repair since a given time
    const connections = [
      connection({
        ...repair,
        checkedAt: "2026-10-21T09:00:00Z",
        package: "invoices-kit",
        remote: true,
      }),
    ];

    // WHEN the rows are built
    const [row] = connectionRows(connections);

    // THEN the row keeps all of it, as given
    expect(row.status).toBe("needs_repair");
    expect(row.reason).toEqual({ key: "unreachable" });
    expect(row.checkedAt).toBe("2026-10-21T09:00:00Z");
    expect(row.package).toBe("invoices-kit");
    expect(row.remote).toBe(true);
  });

  it("names a server row by the label the screen hands it", () => {
    // GIVEN a server whose catalogue label differs from its name
    const connections = [connection({ id: "mcp:server-filesystem", label: "server-filesystem" })];

    // WHEN the rows are built with a naming function
    const rows = connectionRows(connections, (c) => (c.kind === "mcp" ? "Local files" : c.label));

    // THEN the row shows the label and keeps the server's identifier
    expect(rows[0].label).toBe("Local files");
    expect(rows[0].key).toBe("mcp:server-filesystem");
  });
});

describe("the search and the kind chips", () => {
  const rows = connectionRows([
    google("ada@example.com"),
    connection({ id: "imap:bob@example.com", kind: "imap", label: "Fastmail", account: "bob@example.com" }),
    connection({ id: "mcp:notion", label: "notion", package: "wiki-kit" }),
    connection({ id: "folder:invoices", kind: "folder", label: "Invoices" }),
  ]);

  it("matches the name, the account or the package", () => {
    // GIVEN the Google row and the server brought by a package
    const googleRow = rows.find((r) => r.kind === "native")!;
    const notion = rows.find((r) => r.kind === "mcp")!;

    // WHEN they are searched
    // THEN the name, the account and the package match, another word does not
    expect(rowMatches(googleRow, "goo")).toBe(true);
    expect(rowMatches(googleRow, "ADA@")).toBe(true);
    expect(rowMatches(notion, "wiki")).toBe(true);
    expect(rowMatches(googleRow, "notion")).toBe(false);
    expect(rowMatches(googleRow, "  ")).toBe(true);
  });

  it("lets through the kind of the chip, and every kind with All", () => {
    // GIVEN the four kinds in the list
    // WHEN a chip is chosen
    // THEN only its kind is shown, and All shows them all
    expect(visibleRows(rows, "", "imap").map((r) => r.key)).toEqual(["imap:bob@example.com"]);
    expect(visibleRows(rows, "", "folder").map((r) => r.key)).toEqual(["folder:invoices"]);
    expect(visibleRows(rows, "", "all")).toHaveLength(4);
    expect(visibleRows(rows, "fast", "native")).toEqual([]);
  });

  it("lets through the state count chosen in the summary, with the chip and the search", () => {
    // GIVEN a list where only the server works
    const mixed = connectionRows([
      google("ada@example.com", repair),
      connection({ id: "mcp:notion", label: "notion" }),
      connection({ id: "folder:invoices", kind: "folder", label: "Invoices", status: "paused" }),
    ]);

    // WHEN a state count is chosen
    // THEN Working lets the server through, Needs attention the two others,
    //      and the chip and the search still narrow it
    expect(visibleRows(mixed, "", "all", "working").map((r) => r.key)).toEqual(["mcp:notion"]);
    expect(visibleRows(mixed, "", "all", "action").map((r) => r.key)).toEqual([
      "native:google:ada@example.com",
      "folder:invoices",
    ]);
    expect(visibleRows(mixed, "", "folder", "action").map((r) => r.key)).toEqual(["folder:invoices"]);
    expect(visibleRows(mixed, "inv", "all", "working")).toEqual([]);
    expect(visibleRows(mixed, "", "all", "all")).toHaveLength(3);
  });

  it("counts the rows of each chip", () => {
    // GIVEN one connection of each kind
    // WHEN the chips are counted
    // THEN each kind holds one and All holds four
    expect(kindCounts(rows)).toEqual({ all: 4, native: 1, imap: 1, mcp: 1, folder: 1 });
  });
});

describe("row helpers", () => {
  it("reads the native provider from the identifier, and only for a native connection", () => {
    // GIVEN a Microsoft account and a server whose name looks like a provider
    const microsoft = connection({ id: "native:microsoft:ada@example.com", kind: "native" });
    const server = connection({ id: "mcp:google", label: "google" });

    // WHEN the provider is read
    // THEN the account has one, the server none
    expect(nativeProviderOf(microsoft)).toBe("microsoft");
    expect(nativeProviderOf(server)).toBeNull();
    expect(serverNameOf(server.id)).toBe("google");
    expect(serverNameOf(microsoft.id)).toBeNull();
  });

  it("knows which row the detail card shows", () => {
    // GIVEN a server row and a folder row
    const [server, folder] = connectionRows([
      connection(),
      connection({ id: "folder:invoices", kind: "folder", label: "Invoices" }),
    ]);

    // WHEN the selection points at the folder
    const selection = { kind: "connection", id: "folder:invoices" } as const;

    // THEN the folder is selected, the server is not, nor anything without a selection
    expect(isSelected(folder, selection)).toBe(true);
    expect(isSelected(server, selection)).toBe(false);
    expect(isSelected(server, null)).toBe(false);
    expect(isSelected(server, { kind: "builtin" })).toBe(false);
  });

  it("writes the developer provenance line with the technical kind and the origin", () => {
    // GIVEN an official server of the registry and a custom one
    const [official, custom] = connectionRows([
      connection({ id: "mcp:notion", label: "notion", provenance: "registry", official: true }),
      connection({ id: "mcp:wiki", label: "wiki", provenance: "custom" }),
    ]);

    // WHEN the provenance lines are written
    // THEN they name the kind, where the connection came from, and the official mark
    expect(provenanceLine(official, "Official")).toBe("MCP · registry · Official");
    expect(provenanceLine(custom, "Official")).toBe("MCP · custom");
  });
});
