import { describe, test, expect, vi, beforeEach, afterEach } from "vitest";
import { flushSync, mount, unmount } from "svelte";
import { locale, waitLocale } from "svelte-i18n";

const invoke = vi.hoisted(() => vi.fn(async (..._args: unknown[]): Promise<unknown> => null));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

import "$lib/i18n";
import { developerMode } from "$lib/stores/developerMode";
import type { Connection, ConnectionProvenance, McpToolSummaryView } from "$lib/types";
import McpToolsTab from "./McpToolsTab.svelte";

function service(provenance: ConnectionProvenance): Connection {
  return {
    id: "mcp:acme",
    kind: "mcp",
    label: "Acme",
    account: null,
    provenance,
    remote: true,
    official: false,
    state: "connected",
    status: "working",
    reason: null,
    checkedAt: null,
    package: null,
    tools: [
      { id: "mcp:acme/search", family: "read", familySource: "server", prechecked: true },
      { id: "mcp:acme/create_page", family: "unclassified", familySource: "none", prechecked: false },
    ],
  } as Connection;
}

const RUNNING: McpToolSummaryView[] = [
  { local_name: "search", full_name: "mcp:acme/search", description: "Search pages", input_schema: {} },
  { local_name: "create_page", full_name: "mcp:acme/create_page", description: null, input_schema: {} },
];

let target: HTMLElement;
let tab: ReturnType<typeof mount> | null = null;

function byTestid(id: string): HTMLElement | null {
  return target.querySelector(`[data-testid="${id}"]`);
}

async function mountTab(connection: Connection): Promise<void> {
  tab = mount(McpToolsTab, {
    target,
    props: { tools: RUNNING, serverName: "acme", connection, onupdated: () => {} },
  });
  for (let i = 0; i < 4; i++) {
    await Promise.resolve();
    flushSync();
  }
}

beforeEach(async () => {
  locale.set("en");
  await waitLocale("en");
  developerMode.set(false);
  invoke.mockReset();
  target = document.createElement("div");
  document.body.appendChild(target);
});

afterEach(() => {
  if (tab) unmount(tab);
  tab = null;
  target.remove();
});

describe("the tools of a running service and their family", () => {
  for (const developer of [false, true]) {
    const mode = developer ? "on" : "off";

    test(`each tool says its family and offers Change outside the selection, developer mode ${mode}`, async () => {
      // GIVEN a running service added by its address
      developerMode.set(developer);

      // WHEN its tools are shown
      await mountTab(service("custom"));

      // THEN each tool names its family in plain words and may be changed
      expect(byTestid("tool-family-search")?.textContent?.trim()).toBe("Read");
      expect(byTestid("tool-family-create_page")?.textContent?.trim()).toBe("Unclassified");
      expect(byTestid("tool-family-change-search")).not.toBeNull();
      expect(byTestid("tool-family-change-create_page")).not.toBeNull();
    });

    test(`a running service from the selection shows its families without Change, developer mode ${mode}`, async () => {
      // GIVEN a running service from the selection
      developerMode.set(developer);

      // WHEN its tools are shown
      await mountTab(service("selection"));

      // THEN the families are named and none can be changed
      expect(byTestid("tool-family-search")?.textContent?.trim()).toBe("Read");
      expect(byTestid("tool-family-change-search")).toBeNull();
      expect(byTestid("tool-family-change-create_page")).toBeNull();
    });
  }
});
