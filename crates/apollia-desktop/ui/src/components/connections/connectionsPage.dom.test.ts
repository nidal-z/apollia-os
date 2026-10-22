import { describe, test, expect, vi, beforeEach, afterEach } from "vitest";
import { flushSync, mount, unmount } from "svelte";
import { locale, waitLocale } from "svelte-i18n";

const invoke = vi.hoisted(() => vi.fn(async (..._args: unknown[]): Promise<unknown> => null));
const pickFolder = vi.hoisted(() => vi.fn(async (..._args: unknown[]): Promise<unknown> => null));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: pickFolder }));

import "$lib/i18n";
import { developerMode } from "$lib/stores/developerMode";
import type { Connection } from "$lib/types";
import Connections from "../../routes/Connections.svelte";

// The page is mounted whole in the DOM, its commands answered here: what is
// held is which command a gesture reaches, and that the list is not read again
// where the state found replaces a row.

function connection(over: Partial<Connection>): Connection {
  return {
    id: "imap:ada@example.com",
    kind: "imap",
    label: "ada@example.com",
    account: "ada@example.com",
    provenance: "custom",
    remote: true,
    official: false,
    state: "connected",
    status: "not_checked",
    reason: { key: "not_checked" },
    checkedAt: null,
    package: null,
    tools: [],
    ...over,
  } as Connection;
}

let listed: Connection[] = [];
let answers: Record<string, (args: Record<string, unknown>) => unknown> = {};
let target: HTMLElement;
let page: ReturnType<typeof mount> | null = null;

async function settle(rounds = 20): Promise<void> {
  for (let i = 0; i < rounds; i++) {
    await new Promise((resolve) => setTimeout(resolve, 0));
    flushSync();
  }
}

function byTestid(id: string): HTMLElement | null {
  return target.querySelector(`[data-testid="${CSS.escape(id)}"]`);
}

async function click(id: string): Promise<void> {
  const element = byTestid(id);
  if (!element) throw new Error(`no element ${id}`);
  element.click();
  await settle();
}

function calls(command: string): unknown[] {
  return invoke.mock.calls.filter(([name]) => name === command).map(([, args]) => args);
}

beforeEach(async () => {
  locale.set("en");
  await waitLocale("en");
  developerMode.set(false);
  invoke.mockReset();
  pickFolder.mockReset();
  answers = {};
  invoke.mockImplementation(async (command: unknown, args: unknown) => {
    const name = String(command);
    const answer = answers[name];
    if (answer) return answer((args ?? {}) as Record<string, unknown>);
    if (name === "list_connections") return listed;
    if (/(^|_)list(_|$)/.test(name) || name.startsWith("fetch_")) return [];
    return null;
  });
  target = document.createElement("div");
  document.body.appendChild(target);
});

afterEach(() => {
  if (page) unmount(page);
  page = null;
  target.remove();
});

async function mountPage(): Promise<void> {
  page = mount(Connections, { target, props: {} });
  await settle();
}

describe("Check on the badge of a row", () => {
  test("the state found replaces the row, the list is not read again", async () => {
    // GIVEN the page listing a mailbox never checked
    listed = [connection({})];
    answers.check_connection = () => connection({ status: "working", reason: null, checkedAt: "2026-10-22T09:00:00Z" });
    await mountPage();
    const readsBefore = calls("list_connections").length;

    // WHEN Check is clicked on its badge
    await click("connection-state-imap:ada@example.com-check");

    // THEN the connection is checked once, its row shows the state found, and
    // the list was not read again
    expect(calls("check_connection")).toEqual([{ connectionId: "imap:ada@example.com" }]);
    expect(byTestid("connection-state-imap:ada@example.com-check")).toBeNull();
    expect(calls("list_connections").length).toBe(readsBefore);
  });
});

describe("Choose the folder on a folder that cannot be found", () => {
  test("the folder picked becomes the folder of the same connection", async () => {
    // GIVEN the card of a folder connection whose folder is gone, and a picker answering another one
    const gone = connection({
      id: "folder:f1",
      kind: "folder",
      label: "Clients",
      account: null,
      path: "/Users/ada/Clients",
      remote: false,
      status: "needs_repair",
      reason: { key: "folder_not_found" },
    });
    listed = [gone];
    pickFolder.mockResolvedValue("/Users/ada/Archive/Clients");
    answers.repoint_folder_connection = ({ path }) => ({ ...gone, path });
    answers.check_connection = () => ({ ...gone, path: "/Users/ada/Archive/Clients", status: "working", reason: null });
    await mountPage();
    await click("sidebar-connection-folder:f1");

    // WHEN the repair of its card is clicked
    expect(byTestid("connection-repair-btn")?.dataset.repair).toBe("folder");
    await click("connection-repair-btn");

    // THEN the same connection is pointed at the folder picked, nothing is
    // added, and the card shows the state found
    expect(calls("repoint_folder_connection")).toEqual([
      { connectionId: "folder:f1", path: "/Users/ada/Archive/Clients" },
    ]);
    expect(calls("add_folder_connection")).toEqual([]);
    expect(byTestid("connection-repair-btn")).toBeNull();
  });
});
