import { describe, test, expect, vi, beforeEach, afterEach } from "vitest";
import { flushSync, mount, unmount } from "svelte";
import { get } from "svelte/store";
import { locale, waitLocale } from "svelte-i18n";

const invoke = vi.hoisted(() => vi.fn(async (..._args: unknown[]): Promise<unknown> => null));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

import "$lib/i18n";
import { toasts } from "$lib/components/ui/toast/store";
import { developerMode } from "$lib/stores/developerMode";
import type { ConnectionRemoval } from "$lib/ipc/connections";
import type { Connection } from "$lib/types";
import ConnectionCard from "./ConnectionCard.svelte";

const ID = "folder:ada-documents";

function folder(): Connection {
  return {
    id: ID,
    kind: "folder",
    label: "Documents",
    account: null,
    path: "/Users/ada/Documents",
    provenance: "custom",
    remote: false,
    official: false,
    state: "connected",
    status: "working",
    reason: null,
    checkedAt: null,
    package: null,
    tools: [],
  } as Connection;
}

function answer(dryRun: boolean): ConnectionRemoval {
  return dryRun
    ? {
        connection: ID,
        dryRun: true,
        removed: [
          { kind: "source", name: "Ada notes" },
          { kind: "agent_access", name: "read_file", agent: "Writer" },
        ],
        left: [],
      }
    : {
        connection: ID,
        dryRun: false,
        removed: [{ kind: "source", name: "Ada notes" }],
        left: [{ kind: "source", name: "Ada archive" }],
      };
}

let target: HTMLElement;
let card: ReturnType<typeof mount> | null = null;

async function settle(): Promise<void> {
  for (let i = 0; i < 5; i++) {
    await Promise.resolve();
    flushSync();
  }
}

function byTestid(id: string): HTMLElement | null {
  return target.querySelector(`[data-testid="${id}"]`);
}

async function click(id: string): Promise<void> {
  const element = byTestid(id);
  if (!element) throw new Error(`no element ${id}`);
  element.click();
  await settle();
}

beforeEach(async () => {
  locale.set("en");
  await waitLocale("en");
  developerMode.set(false);
  toasts.set([]);
  invoke.mockReset();
  invoke.mockImplementation(async (command: unknown, args: unknown) => {
    if (command === "remove_connection") {
      const { dryRun } = args as { dryRun: boolean };
      return answer(dryRun);
    }
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

function mountCard(onremoved: () => void): void {
  const noop = () => {};
  card = mount(ConnectionCard, {
    target,
    props: { connection: folder(), label: "Documents", onupdated: noop, onremoved, onreload: noop, onrepair: noop },
  });
  flushSync();
}

function removalCalls(): unknown[] {
  return invoke.mock.calls.filter(([command]) => command === "remove_connection").map(([, args]) => args);
}

describe("Remove asks in the page, after the list of what goes", () => {
  test("the dry run comes first, and nothing is removed before the confirmation", async () => {
    // GIVEN the card of a folder connection
    const onremoved = vi.fn();
    mountCard(onremoved);

    // WHEN Remove is clicked
    await click("connection-remove-btn");

    // THEN the runtime is asked what would go, with a dry run, and nothing more
    expect(removalCalls()).toEqual([{ connectionId: ID, dryRun: true }]);
    expect(onremoved).not.toHaveBeenCalled();
    expect(byTestid("connection-remove-confirm")).not.toBeNull();
  });

  test("the confirmation lists what the dry run found", async () => {
    // GIVEN the card of a folder connection whose removal takes a source and a tool of an agent
    mountCard(() => {});

    // WHEN Remove is clicked
    await click("connection-remove-btn");

    // THEN the confirmation names both, in words
    const items = byTestid("connection-remove-items");
    expect(items).not.toBeNull();
    expect(items?.textContent).toContain("the source Ada notes and its index");
    expect(items?.textContent).toContain("the tool read_file of the agent Writer");
  });

  test("confirming removes for good, then says what could not be erased", async () => {
    // GIVEN the confirmation open on the card of a folder connection
    const onremoved = vi.fn();
    mountCard(onremoved);
    await click("connection-remove-btn");

    // WHEN the removal is confirmed
    await click("connection-remove-confirm-btn");

    // THEN the removal runs once for good after the dry run, the card is gone,
    // and what was left behind is said in a toast that stays visible
    expect(removalCalls()).toEqual([
      { connectionId: ID, dryRun: true },
      { connectionId: ID, dryRun: false },
    ]);
    expect(onremoved).toHaveBeenCalledTimes(1);
    const left = get(toasts).find((toast) => toast["data-testid"] === "connection-remove-left");
    expect(left?.message).toContain("Ada archive");
  });

  test("cancelling keeps the connection", async () => {
    // GIVEN the confirmation open
    const onremoved = vi.fn();
    mountCard(onremoved);
    await click("connection-remove-btn");

    // WHEN it is cancelled
    await click("connection-remove-confirm-cancel");

    // THEN nothing was removed and the confirmation is closed
    expect(removalCalls()).toEqual([{ connectionId: ID, dryRun: true }]);
    expect(onremoved).not.toHaveBeenCalled();
    expect(byTestid("connection-remove-confirm")).toBeNull();
  });
});
