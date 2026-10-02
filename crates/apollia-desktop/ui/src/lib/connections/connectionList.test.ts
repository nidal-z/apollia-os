import { describe, it, expect, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { invoke } from "@tauri-apps/api/core";
import { loadConnections, precheckedToolIds, sortConnections } from "./connectionList";
import type { Connection } from "$lib/types";

function connection(over: Partial<Connection> = {}): Connection {
  return {
    id: "mcp:files",
    kind: "mcp",
    label: "files",
    account: null,
    provenance: "custom",
    remote: false,
    official: false,
    state: "connected",
    tools: [],
    ...over,
  };
}

describe("loadConnections", () => {
  it("reads the list through the list_connections command by default", async () => {
    // GIVEN the command answers one MCP server and one Google account
    vi.mocked(invoke).mockResolvedValueOnce([
      connection(),
      connection({ id: "native:google:ada@example.com", kind: "native", label: "Google" }),
    ]);

    // WHEN the list is loaded with no fetcher
    const state = await loadConnections();

    // THEN the command was called and the account comes first
    expect(invoke).toHaveBeenCalledWith("list_connections");
    expect(state.status).toBe("ready");
    if (state.status !== "ready") return;
    expect(state.connections.map((c) => c.id)).toEqual([
      "native:google:ada@example.com",
      "mcp:files",
    ]);
  });

  it("returns an error state instead of throwing", async () => {
    // GIVEN a fetcher that fails
    const fetcher = () => Promise.reject(new Error("runtime unreachable"));

    // WHEN the list is loaded
    const state = await loadConnections(fetcher);

    // THEN the failure is a state the screen can render
    expect(state).toEqual({ status: "error", message: "runtime unreachable" });
  });
});

describe("sortConnections", () => {
  it("orders by kind, then label, then account, without touching its input", () => {
    // GIVEN connections of every kind, out of order
    const input = [
      connection({ id: "folder:docs", kind: "folder", label: "Docs" }),
      connection({ id: "mcp:notion", label: "notion" }),
      connection({ id: "native:google:b", kind: "native", label: "Google", account: "b" }),
      connection({ id: "mcp:files", label: "files" }),
      connection({ id: "native:google:a", kind: "native", label: "Google", account: "a" }),
    ];

    // WHEN sorted
    const sorted = sortConnections(input);

    // THEN accounts, then servers, then folders, and the input is unchanged
    expect(sorted.map((c) => c.id)).toEqual([
      "native:google:a",
      "native:google:b",
      "mcp:files",
      "mcp:notion",
      "folder:docs",
    ]);
    expect(input[0].id).toBe("folder:docs");
  });
});

describe("precheckedToolIds", () => {
  it("keeps only the tools the runtime offers checked", () => {
    // GIVEN a connection with a prechecked read and an unchecked write
    const c = connection({
      tools: [
        { id: "mcp:files/read", family: "read", familySource: "selection", prechecked: true },
        { id: "mcp:files/write", family: "write_or_delete", familySource: "selection", prechecked: false },
      ],
    });

    // WHEN the prechecked tools are read
    // THEN only the read is listed
    expect(precheckedToolIds(c)).toEqual(["mcp:files/read"]);
  });
});
