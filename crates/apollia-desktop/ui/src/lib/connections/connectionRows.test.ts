import { describe, it, expect } from "vitest";
import {
  connectionRows,
  isSelected,
  nativeProviderOf,
  provenanceLine,
  rowMatches,
  statusOfState,
} from "./connectionRows";
import { sortConnections } from "./connectionList";
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

describe("connectionRows", () => {
  it("puts accounts, mailboxes, servers and folders in one list, in the loader's order", () => {
    // GIVEN the loader's answer: a Google account, a mailbox, a server, a folder
    const connections = sortConnections([
      connection({ id: "folder:invoices", kind: "folder", label: "Invoices" }),
      connection(),
      connection({ id: "imap:ada@example.com", kind: "imap", label: "Fastmail", account: "ada@example.com" }),
      google("ada@example.com"),
    ]);

    // WHEN the rows are built
    const rows = connectionRows(connections);

    // THEN one list holds the four kinds, no section between them
    expect(rows.map((r) => r.kind)).toEqual(["native", "imap", "mcp", "folder"]);
    expect(rows.map((r) => r.selection)).toEqual([
      { kind: "native", provider: "google" },
      { kind: "connection", id: "imap:ada@example.com" },
      { kind: "mcp", name: "files" },
      { kind: "connection", id: "folder:invoices" },
    ]);
  });

  it("folds the accounts of one native provider into one row, at its least usable state", () => {
    // GIVEN two Google accounts, the second to sign in again
    const connections = [
      google("ada@example.com"),
      google("bob@example.com", { state: "needs_reauth" }),
    ];

    // WHEN the rows are built
    const rows = connectionRows(connections);

    // THEN one Google row carries both accounts and asks for attention
    expect(rows).toHaveLength(1);
    expect(rows[0].accounts).toEqual(["ada@example.com", "bob@example.com"]);
    expect(rows[0].selection).toEqual({ kind: "native", provider: "google" });
    expect(rows[0].state).toBe("needs_reauth");
    expect(rows[0].status).toBe("attention");
  });

  it("carries the remote attribute the runtime computed, never guessed", () => {
    // GIVEN a remote server and a local one
    const connections = [
      connection({ id: "mcp:linear", label: "linear", remote: true }),
      connection({ id: "mcp:files", label: "files", remote: false }),
    ];

    // WHEN the rows are built
    const [linear, files] = connectionRows(connections);

    // THEN only the remote one is marked remote
    expect(linear.remote).toBe(true);
    expect(files.remote).toBe(false);
  });

  it("names a server row by the label the screen hands it", () => {
    // GIVEN a server whose catalogue label differs from its name
    const connections = [connection({ id: "mcp:server-filesystem", label: "server-filesystem" })];

    // WHEN the rows are built with a naming function
    const rows = connectionRows(connections, (c) => (c.kind === "mcp" ? "Local files" : c.label));

    // THEN the row shows the label and still selects the server by its name
    expect(rows[0].label).toBe("Local files");
    expect(rows[0].selection).toEqual({ kind: "mcp", name: "server-filesystem" });
  });
});

describe("row helpers", () => {
  it("maps every connection state to a status of the filter chips", () => {
    // GIVEN the five states of a connection
    // WHEN each is mapped
    // THEN connected is active, the two recoverable ones attention, unavailable an error, stopped idle
    expect(statusOfState("connected")).toBe("active");
    expect(statusOfState("degraded")).toBe("attention");
    expect(statusOfState("needs_reauth")).toBe("attention");
    expect(statusOfState("unavailable")).toBe("error");
    expect(statusOfState("stopped")).toBe("idle");
  });

  it("reads the native provider from the identifier, and only for a native connection", () => {
    // GIVEN a Microsoft account and a server whose name looks like a provider
    const microsoft = connection({ id: "native:microsoft:ada@example.com", kind: "native" });
    const server = connection({ id: "mcp:google", label: "google" });

    // WHEN the provider is read
    // THEN the account has one, the server none
    expect(nativeProviderOf(microsoft)).toBe("microsoft");
    expect(nativeProviderOf(server)).toBeNull();
  });

  it("matches the filter on the label or an account", () => {
    // GIVEN a Google row with one account
    const [row] = connectionRows([google("ada@example.com")]);

    // WHEN it is filtered
    // THEN the label and the account both match, another word does not
    expect(rowMatches(row, "goo")).toBe(true);
    expect(rowMatches(row, "ADA@")).toBe(true);
    expect(rowMatches(row, "notion")).toBe(false);
    expect(rowMatches(row, "  ")).toBe(true);
  });

  it("knows which row the detail pane shows", () => {
    // GIVEN a server row and a folder row
    const [server, folder] = connectionRows([
      connection(),
      connection({ id: "folder:invoices", kind: "folder", label: "Invoices" }),
    ]);

    // WHEN the selection points at the folder
    const selection = { kind: "connection", id: "folder:invoices" } as const;

    // THEN the folder is selected and the server is not
    expect(isSelected(folder, selection)).toBe(true);
    expect(isSelected(server, selection)).toBe(false);
    expect(isSelected(server, null)).toBe(false);
  });

  it("writes the developer provenance line with the technical kind and the origin", () => {
    // GIVEN an official server of the registry and a custom one
    const [official, custom] = connectionRows([
      connection({ id: "mcp:notion", label: "notion", provenance: "registry", official: true }),
      connection({ id: "mcp:mine", label: "mine", provenance: "custom" }),
    ]);

    // WHEN the provenance lines are written
    // THEN they name the kind, where the connection came from, and the official mark
    expect(provenanceLine(official, "Official")).toBe("MCP · registry · Official");
    expect(provenanceLine(custom, "Official")).toBe("MCP · custom");
  });
});
