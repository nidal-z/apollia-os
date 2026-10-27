import { describe, test, expect, vi, beforeEach, afterEach } from "vitest";
import { flushSync, mount, unmount } from "svelte";
import { locale, waitLocale } from "svelte-i18n";

type Handler = (event: { payload: unknown }) => void;

const invoke = vi.hoisted(() => vi.fn(async (..._args: unknown[]): Promise<unknown> => null));
const handlers = vi.hoisted(() => new Map<string, Handler[]>());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (name: string, handler: Handler) => {
    handlers.set(name, [...(handlers.get(name) ?? []), handler]);
    return () => handlers.set(name, (handlers.get(name) ?? []).filter((h) => h !== handler));
  }),
}));

import "$lib/i18n";
import { developerMode } from "$lib/stores/developerMode";
import type { Connection } from "$lib/types";
import Connections from "../../routes/Connections.svelte";

// The page is mounted whole in the DOM, its commands answered here and its
// runtime events sent from here: no Microsoft endpoint is reached.

const ADDRESS = "https://login.microsoftonline.com/organizations/adminconsent?client_id=c4f95bc5";

function blockedMicrosoft(): Connection {
  return {
    id: "native:microsoft",
    kind: "native",
    label: "Microsoft",
    account: null,
    provenance: "selection",
    remote: true,
    official: false,
    state: "stopped",
    status: "blocked",
    reason: { key: "admin_approval_required", detail: "AADSTS90094", address: ADDRESS },
    checkedAt: "2026-10-27T09:00:00Z",
    package: null,
    tools: [],
  } as Connection;
}

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

function sendRuntimeEvent(eventType: string, payload: unknown): void {
  for (const handler of handlers.get("runtime-event") ?? []) {
    handler({ payload: { event_type: eventType, payload: { [eventType]: payload } } });
  }
}

beforeEach(async () => {
  locale.set("en");
  await waitLocale("en");
  developerMode.set(false);
  handlers.clear();
  invoke.mockReset();
  invoke.mockImplementation(async (command: unknown) => {
    const name = String(command);
    if (name === "list_connections") return [blockedMicrosoft()];
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

async function openBlockedCard(): Promise<void> {
  page = mount(Connections, { target, props: {} });
  await settle();
  await click("sidebar-connection-native:microsoft");
}

describe("the card of a Microsoft account blocked until an administrator approves", () => {
  test("gives the approval address and the request to copy, and Retry, not the rule", async () => {
    // GIVEN the page listing Microsoft blocked with its approval page kept
    // WHEN its card is opened
    await openBlockedCard();

    // THEN the card shows the administrator's approval: the address kept, the
    // request written with it, each to copy
    expect(byTestid("oauth-admin-approval")).not.toBeNull();
    expect((byTestid("oauth-admin-approval-address") as HTMLInputElement).value).toBe(ADDRESS);
    expect((byTestid("oauth-admin-approval-request") as HTMLTextAreaElement).value).toContain(ADDRESS);
    expect(byTestid("oauth-admin-approval-copy")).not.toBeNull();
    expect(byTestid("oauth-admin-approval-request-copy")).not.toBeNull();
    // AND its one action is Retry, the rule is not offered
    const repair = byTestid("connection-repair-btn");
    expect(repair?.dataset.repair).toBe("admin_approval");
    expect(repair?.textContent).toContain("Retry");
    expect(target.querySelectorAll('[data-testid="connection-repair-btn"]')).toHaveLength(1);
    expect(target.textContent).not.toContain("See the rule");
  });

  test("Retry starts the Microsoft connection again", async () => {
    // GIVEN the card of Microsoft blocked until an administrator approves
    await openBlockedCard();

    // WHEN Retry is clicked
    await click("connection-repair-btn");

    // THEN the connection dialog of Microsoft opens, on the card's page
    const dialog = document.body.querySelector('[data-testid="oauth-dialog"]');
    expect(dialog?.getAttribute("aria-label")).toBe("Connect Microsoft 365");
    expect(byTestid("connector-detail")).not.toBeNull();
  });

  test("once an account connected, the block lifted shows on the card without reading the list again", async () => {
    // GIVEN the card of Microsoft blocked until an administrator approves
    await openBlockedCard();
    expect(byTestid("oauth-admin-approval")).not.toBeNull();
    const readsBefore = calls("list_connections").length;

    // WHEN the runtime announces the block lifted, as it does once an account connected
    sendRuntimeEvent("ConnectionStateChanged", {
      id: "native:microsoft",
      state: "working",
      reason: null,
      checked_at: "2026-10-27T09:05:00Z",
    });
    await settle();

    // THEN the approval and its Retry are gone from the card, and the list was not read again
    expect(byTestid("oauth-admin-approval")).toBeNull();
    expect(byTestid("connection-repair-btn")).toBeNull();
    expect(byTestid("connection-detail-state")?.textContent).toContain("Working");
    expect(calls("list_connections").length).toBe(readsBefore);
  });
});
