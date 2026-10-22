import { describe, test, expect, vi, beforeEach, afterEach } from "vitest";
import { flushSync, mount, unmount } from "svelte";
import { locale, waitLocale } from "svelte-i18n";

const invoke = vi.hoisted(() => vi.fn(async (..._args: unknown[]): Promise<unknown> => null));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

import "$lib/i18n";
import { developerMode } from "$lib/stores/developerMode";
import type { McpServerRawConfig } from "$lib/ipc/connections";
import type { Connection } from "$lib/types";
import ConnectionCard from "./ConnectionCard.svelte";

function service(over: Partial<Connection> = {}): Connection {
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
    checkedAt: "2026-10-22T08:00:00Z",
    package: null,
    tools: [],
    ...over,
  } as Connection;
}

const REMOTE: McpServerRawConfig = {
  name: "notion",
  command: "",
  args: [],
  env: { Authorization: "${APOLLIA_OAUTH}", "X-Api-Key": "${APOLLIA_SECRET:X-Api-Key}" },
  transport: "streamable-http",
  url: "https://mcp.notion.example/mcp",
  init_timeout_secs: 30,
  call_timeout_secs: 60,
  tags: [],
};

const LOCAL: McpServerRawConfig = {
  name: "files",
  command: "npx",
  args: ["-y", "@modelcontextprotocol/server-filesystem", "/Users/ada/Documents"],
  env: {},
  transport: "stdio",
  url: null,
  init_timeout_secs: 30,
  call_timeout_secs: 60,
  tags: [],
};

let raw: McpServerRawConfig = REMOTE;
let target: HTMLElement;
let card: ReturnType<typeof mount> | null = null;

async function settle(): Promise<void> {
  for (let i = 0; i < 10; i++) {
    await new Promise((resolve) => setTimeout(resolve, 0));
    flushSync();
  }
}

function byTestid(id: string): HTMLElement | null {
  return target.querySelector(`[data-testid="${CSS.escape(id)}"]`);
}

function calls(command: string): unknown[] {
  return invoke.mock.calls.filter(([name]) => name === command).map(([, args]) => args);
}

beforeEach(async () => {
  locale.set("en");
  await waitLocale("en");
  developerMode.set(false);
  raw = REMOTE;
  invoke.mockReset();
  invoke.mockImplementation(async (command: unknown) => {
    if (command === "get_mcp_server_raw_config") return raw;
    if (command === "mcp_oauth_login") return { account: "ada" };
    if (command === "check_connection") return service();
    if (/(^|_)list(_|$)/.test(String(command))) return [];
    return null;
  });
  target = document.createElement("div");
  document.body.appendChild(target);
});

afterEach(() => {
  if (card) unmount(card);
  card = null;
  target.remove();
});

interface Handlers {
  onupdated?: (c: Connection) => void;
  onrepair?: (kind: string) => void;
  onaddaccount?: () => void;
}

async function mountCard(connection: Connection, handlers: Handlers = {}): Promise<void> {
  const noop = () => {};
  card = mount(ConnectionCard, {
    target,
    props: {
      connection,
      label: connection.label,
      onupdated: handlers.onupdated ?? noop,
      onremoved: noop,
      onreload: noop,
      onrepair: handlers.onrepair ?? noop,
      onaddaccount: handlers.onaddaccount,
    },
  });
  await settle();
}

describe("a service that asks to sign in again", () => {
  test("its repair signs in again through its own OAuth, then checks it", async () => {
    // GIVEN the card of a remote service whose sign-in expired
    const onupdated = vi.fn();
    const onrepair = vi.fn();
    await mountCard(service({ status: "needs_repair", reason: { key: "sign_in_again" } }), { onupdated, onrepair });

    // WHEN its one repair is clicked
    const repair = byTestid("connection-repair-btn");
    expect(repair?.dataset.repair).toBe("sign_in");
    repair?.click();
    await settle();

    // THEN the OAuth of the service runs at its address, nothing is restarted,
    // and the state found is handed back
    expect(calls("mcp_oauth_login")).toEqual([
      { serverName: "notion", serverUrl: "https://mcp.notion.example/mcp", wwwAuthenticate: null, scopes: [] },
    ]);
    expect(calls("restart_mcp_server")).toEqual([]);
    expect(onrepair).not.toHaveBeenCalled();
    expect(calls("check_connection")).toEqual([{ connectionId: "mcp:notion" }]);
    expect(onupdated).toHaveBeenCalledTimes(1);
  });
});

describe("the settings of a service, developer mode off", () => {
  test("the address, the secrets and the sign-in are there; the transport is not", async () => {
    // GIVEN the card of a remote service, developer mode off
    raw = REMOTE;
    await mountCard(service());

    // THEN its address, its secret and its sign-in again can be changed, and
    // its transport is kept for the developer mode
    expect(byTestid("mcp-settings-url-input")).not.toBeNull();
    expect(byTestid("mcp-settings-secret-X-Api-Key")).not.toBeNull();
    expect(byTestid("mcp-settings-oauth-reconnect")).not.toBeNull();
    expect(byTestid("mcp-settings-transport")).toBeNull();
    expect(byTestid("connection-technical")).toBeNull();
  });

  test("the arguments of a local service are there; its command is not", async () => {
    // GIVEN the card of a local service, developer mode off
    raw = LOCAL;
    await mountCard(service({ id: "mcp:files", label: "Files", remote: false }));

    // THEN its arguments can be changed, and its command line is kept for the developer mode
    expect(byTestid("mcp-settings-args-input")).not.toBeNull();
    expect(byTestid("mcp-settings-command")).toBeNull();
  });

  test("in developer mode the transport and the command come back", async () => {
    // GIVEN developer mode on
    developerMode.set(true);
    raw = REMOTE;
    await mountCard(service());

    // THEN the transport of the remote service shows
    expect(byTestid("mcp-settings-transport")).not.toBeNull();
  });
});

describe("the card of an account", () => {
  test("offers to connect another account of the same provider", async () => {
    // GIVEN the card of a Google account
    const onaddaccount = vi.fn();
    await mountCard(
      service({
        id: "native:google:ada@example.com",
        kind: "native",
        label: "Google",
        account: "ada@example.com",
      }),
      { onaddaccount },
    );

    // WHEN Add another account is clicked
    const add = byTestid("oauth-add-account-google");
    expect(add?.textContent).toContain("Add another account");
    add?.click();
    await settle();

    // THEN the page is asked to connect another account
    expect(onaddaccount).toHaveBeenCalledTimes(1);
  });
});
