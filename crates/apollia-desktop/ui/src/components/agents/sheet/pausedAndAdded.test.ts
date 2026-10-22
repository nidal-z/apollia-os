import { beforeAll, describe, expect, test, vi } from "vitest";
import { render as renderToString } from "svelte/server";
import type { Component } from "svelte";
import { locale, waitLocale } from "svelte-i18n";
import "$lib/i18n";

// Stores read `localStorage` at import time; the node environment has none.
vi.hoisted(() => {
  const memory = new Map<string, string>();
  globalThis.localStorage = {
    getItem: (k: string) => memory.get(k) ?? null,
    setItem: (k: string, v: string) => void memory.set(k, v),
    removeItem: (k: string) => void memory.delete(k),
    clear: () => memory.clear(),
    key: (i: number) => Array.from(memory.keys())[i] ?? null,
    get length() {
      return memory.size;
    },
  };
});

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => null) }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(async () => null) }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
  emit: vi.fn(async () => {}),
}));

import type { Connection, ConnectionTool } from "$lib/types";
import SheetAccess from "./SheetAccess.svelte";
import SheetSources from "./SheetSources.svelte";
import AdjustSources from "./AdjustSources.svelte";
import type { Source } from "$lib/ipc/sources";
import type { AgentSources } from "./useAgentSources.svelte";
import { accessOrigin, accessView, groupAccesses } from "./agentSheet";
import { pausedAccesses } from "./connectionGrant";
import { pausedSources, type SourceRow } from "./agentSources";

/** The markup `component` draws with `props`, rendered on the server. */
function html<P extends Record<string, unknown>>(component: Component<P>, props: P): string {
  return renderToString(component as Component<Record<string, unknown>>, { props }).body;
}

function tool(id: string): ConnectionTool {
  return { id, family: "read", familySource: "selection", prechecked: true };
}

function connection(over: Partial<Connection>): Connection {
  return {
    id: "mcp:notion",
    kind: "mcp",
    label: "Notion",
    account: null,
    provenance: "selection",
    remote: true,
    official: true,
    state: "connected",
    status: "working",
    reason: null,
    checkedAt: null,
    package: null,
    tools: [],
    ...over,
  };
}

function row(over: Partial<SourceRow>): SourceRow {
  return {
    id: "src-1",
    kind: "folder",
    label: "Invoices",
    checked: true,
    state: "ready",
    documents: 3,
    tooLarge: 0,
    unreadable: 0,
    skipped: [],
    reference: "folder:invoices",
    registered: true,
    ...over,
  };
}

/** The opening tag of the access row of `id`, and what it holds up to the next row. */
function accessRow(markup: string, label: string): string {
  const at = markup.indexOf(label);
  const start = markup.lastIndexOf("<li", at);
  const end = markup.indexOf("</li>", at);
  return markup.slice(start, end);
}

beforeAll(async () => {
  locale.set("en");
  await waitLocale();
});

describe("a paused connection on the sheet of an agent", () => {
  const notion = connection({ status: "paused", tools: [tool("mcp:notion/search")] });
  const files = connection({ id: "mcp:files", label: "Files", remote: false, tools: [tool("mcp:files/read")] });

  test("only the accesses of the paused connection are marked", () => {
    // GIVEN an agent holding a tool of a paused connection, one of a working
    //       connection, and a native tool
    const ids = ["mcp:notion/search", "mcp:files/read", "file_read"];

    // WHEN the paused accesses are computed
    const paused = pausedAccesses(ids, [notion, files]);

    // THEN only the tool of the paused connection is marked
    expect([...paused]).toEqual(["mcp:notion/search"]);
  });

  test("an access of a paused connection carries the Paused badge, like Remote", () => {
    // GIVEN the access section of an agent holding the tool of a paused
    //       remote connection and a native tool
    const groups = groupAccesses([
      accessView("mcp:notion/search", { description: "Search Notion", family: "read", family_source: "server", kind: "mcp" }),
      accessView("file_read", { description: "Read a file", family: "read", family_source: "builtin", kind: "native" }),
    ]);

    // WHEN the section is drawn
    const markup = html(SheetAccess, {
      groups,
      loading: false,
      remote: new Set(["mcp:notion/search"]),
      paused: new Set(["mcp:notion/search"]),
    });

    // THEN the row of the paused tool is marked and reads Remote and Paused,
    //      the native tool reads neither
    const paused = accessRow(markup, "Search Notion");
    expect(paused).toContain('data-paused="true"');
    expect(paused).toContain("Remote");
    expect(paused).toContain("Paused");
    const native = accessRow(markup, "Read a file");
    expect(native).not.toContain("data-paused");
    expect(native).not.toContain("Paused");
  });

  test("a mailbox and a folder read through a paused connection carry the same mark", () => {
    // GIVEN a paused IMAP mailbox, a paused folder, an Outlook account at work,
    //       and the sources an agent reads through them, a file of the
    //       paused folder among them
    const connections = [
      connection({ id: "imap:ada@example.com", kind: "imap", status: "paused" }),
      connection({ id: "folder:invoices", kind: "folder", path: "/home/ada/invoices", status: "paused" }),
      connection({ id: "native:microsoft:ada@example.com", kind: "native" }),
    ];
    const rows = [
      row({ id: "mail-imap", kind: "mail", reference: "imap:ada@example.com" }),
      row({ id: "folder", kind: "folder", reference: "folder:invoices" }),
      row({ id: "file", kind: "file", reference: "/home/ada/invoices/march.pdf" }),
      row({ id: "mail-outlook", kind: "mail", reference: "outlook:ada@example.com" }),
    ];

    // WHEN the paused sources are computed, and the section is drawn
    const paused = pausedSources(rows, connections);
    const markup = html(SheetSources, { rows, paused });

    // THEN the mailbox, the folder and its file are marked, the Outlook
    //      account is not, and each marked row reads Paused
    expect([...paused].sort()).toEqual(["file", "folder", "mail-imap"]);
    expect(markup.match(/data-paused="true"/g)?.length).toBe(3);
    expect(markup.match(/>Paused</g)?.length).toBe(3);
  });

  test("in the adjustment, a source read through a paused connection carries the same mark", () => {
    // GIVEN a registry holding a folder of a paused connection, a mailbox at
    //       work, and the connections the sheet already read
    const source = (id: string, kind: Source["kind"], reference: string): Source => ({
      id,
      kind,
      label: id,
      group: null,
      reference,
      indexState: "ready",
      addedAt: "2026-10-22T00:00:00Z",
    });
    const sources: AgentSources = {
      registry: [source("folder", "folder", "folder:invoices"), source("mail-imap", "mail", "imap:ada@example.com")],
      indexes: {},
      error: null,
      refresh: async () => {},
      add: async () => null,
      remove: async () => false,
    };
    const known = [
      connection({ id: "folder:invoices", kind: "folder", path: "/home/ada/invoices", status: "paused" }),
      connection({ id: "imap:ada@example.com", kind: "imap" }),
    ];
    const draft = { mission: "", examples: [], access: [], sources: ["folder", "mail-imap"], model: null };

    // WHEN the sources are drawn for adjustment
    const markup = html(AdjustSources, { draft, sources, modified: false, known });

    // THEN only the folder of the paused connection is marked and reads Paused
    expect(accessRow(markup, 'id="agent-adjust-source-folder"')).toContain('data-paused="true"');
    expect(accessRow(markup, 'id="agent-adjust-source-mail-imap"')).not.toContain("data-paused");
    expect(markup.match(/data-paused="true"/g)?.length).toBe(1);
    expect(markup.match(/>Paused</g)?.length).toBe(1);
  });
});

describe("an agent on the default access with accesses on top", () => {
  test("only an agent on the default access reads what it was given on top", () => {
    // GIVEN an agent on the default access given a tool on top, and one with a
    //       chosen list the runtime would never mark
    const onDefault = { access_by_default: true, access_added: ["mcp:notion/search"] };
    const listed = { access_by_default: false, access_added: ["mcp:notion/search"] };

    // WHEN their origins are read
    // THEN the first holds the default access and the tool on top, the second
    //      neither, and an older runtime that says nothing reads as listed
    expect(accessOrigin(onDefault)).toEqual({ byDefault: true, added: new Set(["mcp:notion/search"]) });
    expect(accessOrigin(listed)).toEqual({ byDefault: false, added: new Set() });
    expect(accessOrigin({})).toEqual({ byDefault: false, added: new Set() });
  });

  test("the section says Default and marks the access given On top", () => {
    // GIVEN the access section of an agent on the default access, given the
    //       search of Notion on top of the reading of files
    const groups = groupAccesses([
      accessView("mcp:notion/search", { description: "Search Notion", family: "read", family_source: "server", kind: "mcp" }),
      accessView("file_read", { description: "Read a file", family: "read", family_source: "builtin", kind: "native" }),
    ]);
    const origin = accessOrigin({ access_by_default: true, access_added: ["mcp:notion/search"] });

    // WHEN the section is drawn, then the same section for a chosen list
    const markup = html(SheetAccess, { groups, loading: false, origin });
    const chosen = html(SheetAccess, { groups, loading: false });

    // THEN it reads Default with one access on top, the added tool reads On
    //      top and the default one does not; a chosen list reads neither
    expect(markup).toContain('data-access-mode="default"');
    expect(markup).toContain(">Default<");
    expect(markup).toContain("and one access on top.");
    const added = accessRow(markup, "Search Notion");
    expect(added).toContain('data-added="true"');
    expect(added).toContain("On top");
    expect(accessRow(markup, "Read a file")).not.toContain("On top");
    expect(chosen).toContain('data-access-mode="listed"');
    expect(chosen).not.toContain(">Default<");
    expect(chosen).not.toContain("On top");
  });
});
