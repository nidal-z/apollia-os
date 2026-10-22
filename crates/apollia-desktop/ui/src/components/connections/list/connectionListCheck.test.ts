import { describe, test, expect, vi } from "vitest";
import { render as renderToString } from "svelte/server";
import { locale, waitLocale } from "svelte-i18n";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => null) }));

// Stores read the browser storage at import; node has none.
vi.hoisted(() => {
  const map = new Map<string, string>();
  globalThis.localStorage = {
    getItem: (k: string) => map.get(k) ?? null,
    setItem: (k: string, v: string) => void map.set(k, v),
    removeItem: (k: string) => void map.delete(k),
    clear: () => map.clear(),
    key: () => null,
    length: 0,
  } as Storage;
});

import "$lib/i18n";
import { connectionRows } from "$lib/connections/connectionRows";
import ConnectionList from "./ConnectionList.svelte";
import type { Connection } from "$lib/types";

function connection(over: Partial<Connection>): Connection {
  return {
    id: "mcp:linear",
    kind: "mcp",
    label: "linear",
    account: null,
    provenance: "registry",
    remote: false,
    official: false,
    state: "connected",
    status: "working",
    reason: null,
    checkedAt: "2026-10-22T08:00:00Z",
    package: null,
    tools: [],
    ...over,
  };
}

async function renderList(connections: Connection[]): Promise<string> {
  locale.set("en");
  await waitLocale("en");
  const rows = connectionRows(connections);
  const noop = () => {};
  return renderToString(ConnectionList as never, {
    props: {
      rows,
      query: "",
      kind: "all",
      selection: null,
      loading: false,
      builtinTools: { active: 0, total: 0 },
      onselect: noop,
      onadd: noop,
      oncheck: noop,
    } as never,
  }).body;
}

describe("the badge of a connection never checked", () => {
  test("offers Check, and only on that connection", async () => {
    // GIVEN a mailbox never checked and a server that works
    const never = connection({
      id: "imap:ada@example.com",
      kind: "imap",
      label: "ada@example.com",
      status: "not_checked",
      reason: { key: "not_checked" },
      checkedAt: null,
    });
    const working = connection({});

    // WHEN the list is rendered
    const html = await renderList([never, working]);

    // THEN the badge of the mailbox carries Check, the server's does not
    expect(html).toContain('data-testid="connection-state-imap:ada@example.com-check"');
    expect(html).not.toContain('data-testid="connection-state-mcp:linear-check"');
    expect(html).toMatch(/connection-state-imap:ada@example.com-check"[^>]*>[\s\S]*?Check/);
  });
});
