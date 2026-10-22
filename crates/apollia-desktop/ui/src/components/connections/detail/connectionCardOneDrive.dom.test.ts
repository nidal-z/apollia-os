import { describe, test, expect, vi, beforeEach, afterEach } from "vitest";
import { flushSync, mount, unmount } from "svelte";
import { locale, waitLocale } from "svelte-i18n";

const invoke = vi.hoisted(() => vi.fn(async (..._args: unknown[]): Promise<unknown> => null));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

import "$lib/i18n";
import { developerMode } from "$lib/stores/developerMode";
import type { Connection } from "$lib/types";
import ConnectionCard from "./ConnectionCard.svelte";

function account(provider: string, email: string): Connection {
  return {
    id: `native:${provider}:${email}`,
    kind: "native",
    label: provider,
    account: email,
    provenance: "selection",
    remote: true,
    official: true,
    state: "connected",
    status: "working",
    reason: null,
    checkedAt: "2026-10-22T08:00:00Z",
    package: null,
    tools: [],
  } as Connection;
}

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

async function mountCard(connection: Connection): Promise<void> {
  const noop = () => {};
  card = mount(ConnectionCard, {
    target,
    props: {
      connection,
      label: connection.label,
      onupdated: noop,
      onremoved: noop,
      onreload: noop,
      onrepair: noop,
    },
  });
  await settle();
}

beforeEach(async () => {
  locale.set("en");
  await waitLocale("en");
  developerMode.set(false);
  invoke.mockReset();
  invoke.mockImplementation(async (command: unknown) => {
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

describe("the OneDrive choices on the card of an account", () => {
  test("a Microsoft account shows the OneDrive files Apollia can read", async () => {
    // GIVEN the card of a connected Microsoft account
    const connection = account("microsoft", "ada@example.com");

    // WHEN it is shown
    await mountCard(connection);

    // THEN its OneDrive choices are on the card, read for that account
    expect(byTestid("onedrive-choices-ada@example.com")).not.toBeNull();
    expect(calls("onedrive_list_choices")).toEqual([{ accountId: "ada@example.com" }]);
  });

  test("a Google account shows no OneDrive choices", async () => {
    // GIVEN the card of a connected Google account
    const connection = account("google", "ada@example.com");

    // WHEN it is shown
    await mountCard(connection);

    // THEN nothing of OneDrive is on it, and nothing of OneDrive is read
    expect(byTestid("onedrive-choices-ada@example.com")).toBeNull();
    expect(calls("onedrive_list_choices")).toEqual([]);
  });
});
