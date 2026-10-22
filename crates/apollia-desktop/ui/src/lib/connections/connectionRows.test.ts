import { describe, it, expect } from "vitest";
import {
  connectionRows,
  isSelected,
  kindCounts,
  nativeProviderOf,
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

  it("puts what needs repair first, then the rest by name", () => {
    // GIVEN a working server, a paused one, a blocked one and two to repair
    const connections = [
      connection({ id: "mcp:alpha", label: "alpha" }),
      connection({ id: "mcp:zeta", label: "zeta", ...repair }),
      connection({ id: "mcp:beta", label: "beta", status: "paused" }),
      connection({ id: "mcp:gamma", label: "gamma", status: "blocked" }),
      connection({ id: "mcp:delta", label: "delta", ...repair }),
    ];

    // WHEN the rows are built
    const rows = connectionRows(connections);

    // THEN the two to repair lead, by name, and every other state follows by name
    expect(rows.map((r) => r.label)).toEqual(["delta", "zeta", "alpha", "beta", "gamma"]);
  });

  it("ranks a connection not checked yet with the rest, by name, never with what needs repair", () => {
    // GIVEN one server to repair, two never checked and one working
    const never = { status: "not_checked", reason: { key: "not_checked" } } as const;
    const connections = [
      connection({ id: "mcp:omega", label: "omega", ...repair }),
      connection({ id: "mcp:alpha", label: "alpha", ...never }),
      connection({ id: "mcp:kappa", label: "kappa" }),
      connection({ id: "mcp:beta", label: "beta", ...never }),
    ];

    // WHEN the rows are built
    const rows = connectionRows(connections);

    // THEN only the one to repair leads; the two never checked sit among the rest by name
    expect(rows.map((r) => r.label)).toEqual(["omega", "alpha", "beta", "kappa"]);
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
