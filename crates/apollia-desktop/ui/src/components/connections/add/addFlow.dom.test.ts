import { describe, test, expect, vi, beforeEach, afterEach } from "vitest";
import { flushSync, mount, unmount } from "svelte";
import { locale, waitLocale } from "svelte-i18n";

const invoke = vi.hoisted(() => vi.fn(async (..._args: unknown[]): Promise<unknown> => null));
const pickFolder = vi.hoisted(() => vi.fn(async (..._args: unknown[]): Promise<unknown> => null));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: pickFolder }));

import "$lib/i18n";
import en from "$lib/i18n/en.json";
import { developerMode } from "$lib/stores/developerMode";
import type { AddEntry } from "$lib/connections/addWindow";
import type { Connection } from "$lib/types";
import AddFlow from "./AddFlow.svelte";
import Connections from "../../../routes/Connections.svelte";

// The dialogs of the page run Svelte transitions, which jsdom cannot play: an
// animation that finishes at once stands in for them.
Element.prototype.animate = function (): Animation {
  const animation = {
    onfinish: null as (() => void) | null,
    cancel: () => {},
    finished: Promise.resolve(),
  };
  setTimeout(() => animation.onfinish?.(), 0);
  return animation as unknown as Animation;
};

// The flow and the page are mounted in the DOM, their commands answered here:
// what is held is what a gesture leads to on screen, and which command it reaches.

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
    status: "working",
    reason: null,
    checkedAt: null,
    package: null,
    tools: [],
    ...over,
  } as Connection;
}

const FOLDER = connection({
  id: "folder:clients",
  kind: "folder",
  label: "Clients",
  account: null,
  path: "/Users/ada/Clients",
  remote: false,
  tools: [{ id: "file_read", family: "read", familySource: "server", prechecked: false }],
} as Partial<Connection>);

let listed: Connection[] = [];
let answers: Record<string, (args: Record<string, unknown>) => unknown> = {};
let target: HTMLElement;
let mounted: ReturnType<typeof mount> | null = null;

async function settle(rounds = 20): Promise<void> {
  for (let i = 0; i < rounds; i++) {
    await new Promise((resolve) => setTimeout(resolve, 0));
    flushSync();
  }
}

/** Dialogs may render outside the mount target, so the whole document is searched. */
function byTestid(id: string): HTMLElement | null {
  return document.querySelector(`[data-testid="${CSS.escape(id)}"]`);
}

async function click(id: string): Promise<void> {
  const element = byTestid(id);
  if (!element) throw new Error(`no element ${id}`);
  element.click();
  await settle();
}

/** A row of the add window: its testid sits on the item, the gesture on its button. */
async function choose(id: string): Promise<void> {
  const button = byTestid(id)?.querySelector<HTMLElement>('[role="button"]');
  if (!button) throw new Error(`no row ${id}`);
  button.click();
  await settle();
}

async function type(id: string, value: string): Promise<void> {
  const input = byTestid(id) as HTMLInputElement | null;
  if (!input) throw new Error(`no input ${id}`);
  input.value = value;
  input.dispatchEvent(new Event("input", { bubbles: true }));
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
  listed = [];
  answers = {};
  invoke.mockImplementation(async (command: unknown, args: unknown) => {
    const name = String(command);
    const answer = answers[name];
    if (answer) return answer((args ?? {}) as Record<string, unknown>);
    if (name === "list_connections") return listed;
    if (name === "list_imap_providers") return { providers: [], defaultPort: 993 };
    if (/(^|_)list(_|$)/.test(name) || name.startsWith("fetch_")) return [];
    return null;
  });
  target = document.createElement("div");
  document.body.appendChild(target);
});

afterEach(() => {
  if (mounted) unmount(mounted);
  mounted = null;
  target.remove();
});

async function mountFlow(entry: AddEntry, name: string): Promise<void> {
  mounted = mount(AddFlow, {
    target,
    props: { entry, name, onback: () => {}, ondone: () => {}, onopenrule: () => {} },
  });
  await settle();
}

async function mountPage(): Promise<void> {
  mounted = mount(Connections, { target, props: {} });
  await settle();
}

describe("a folder in the flow", () => {
  test("the trial of a folder added opens the picker again", async () => {
    // GIVEN the flow of a folder, a picker answering a folder, and the folder added
    pickFolder.mockResolvedValue("/Users/ada/Clients");
    answers.add_folder_connection = () => FOLDER;
    listed = [FOLDER];
    await mountFlow({ key: "folder", kind: "folder" }, "Folder");
    await click("add-flow-folder-pick");
    await click("add-flow-who-continue");
    expect(byTestid("test-result-success")).not.toBeNull();

    // WHEN "Choose another folder" is clicked on the trial
    await click("folder-connection-choose-another");

    // THEN the system picker opens a second time
    expect(pickFolder).toHaveBeenCalledTimes(2);
  });

  test("a folder that could not be added offers the picker again", async () => {
    // GIVEN the flow of a folder whose addition fails
    pickFolder.mockResolvedValue("/Users/ada/Clients");
    answers.add_folder_connection = () => {
      throw new Error("the folder could not be read");
    };
    await mountFlow({ key: "folder", kind: "folder" }, "Folder");
    await click("add-flow-folder-pick");
    expect(byTestid("wizard-finalize-error")).not.toBeNull();

    // WHEN "Choose another folder" is clicked under the failure
    await click("folder-connection-choose-another");

    // THEN the system picker opens a second time, and the addition is tried on it
    expect(pickFolder).toHaveBeenCalledTimes(2);
    expect(calls("add_folder_connection")).toHaveLength(2);
  });
});

describe("a mailbox in the flow", () => {
  test("the trial of a mailbox added says it is only read", async () => {
    // GIVEN the flow of a mailbox, and the mailbox added
    answers.add_imap_mailbox = () => ({ connectionId: "imap:ada@example.com", email: "ada@example.com" });
    listed = [connection({ tools: [{ id: "mail_search", family: "read", familySource: "server", prechecked: false }] } as Partial<Connection>)];
    await mountFlow({ key: "mailbox", kind: "mailbox" }, "Mailbox");
    await type("mailbox-email", "ada@example.com");
    await type("mailbox-password", "app-password");

    // WHEN it is added and who may use it confirmed
    await click("mailbox-add");
    await click("add-flow-who-continue");

    // THEN the trial says nothing is sent from it or deleted
    expect(byTestid("test-result-success")).not.toBeNull();
    expect(byTestid("add-flow-mailbox-read-only")?.textContent?.trim()).toBe(en.connections.mailbox.read_only);
  });
});

describe("the key of a service by its address", () => {
  test("a refused attempt keeps the key, so Retry sends it again", async () => {
    // GIVEN the flow of an address with a key, whose first addition fails
    let attempts = 0;
    answers.add_mcp_server = () => {
      attempts += 1;
      if (attempts === 1) throw new Error("the service did not answer");
      return null;
    };
    await mountFlow({ key: "address", kind: "address" }, "Address");
    await type("custom-mcp-url-input", "https://tools.example.com/connect");
    await click("add-flow-key-toggle");
    await type("add-flow-key-input", "k-123");
    await click("custom-mcp-install-btn");
    expect(byTestid("wizard-finalize-error")).not.toBeNull();

    // WHEN Retry is clicked
    await click("wizard-retry-add-btn");

    // THEN the second addition carries the key as the first did, and the flow goes on
    const sent = calls("add_mcp_server") as Array<{ config: { env: Record<string, string> } }>;
    expect(sent).toHaveLength(2);
    expect(sent[1].config.env.Authorization).toBe("Bearer k-123");
    expect(byTestid("add-flow-who")).not.toBeNull();
  });
});

describe("the page around the flow", () => {
  test("the repair of a refused password opens the flow of a mailbox, its address filled in", async () => {
    // GIVEN the card of a mailbox whose password was refused
    listed = [connection({ status: "needs_repair", reason: { key: "password_refused" } })];
    await mountPage();
    await click("sidebar-connection-imap:ada@example.com");

    // WHEN its repair is clicked
    expect(byTestid("connection-repair-btn")?.dataset.repair).toBe("password");
    await click("connection-repair-btn");

    // THEN the one flow opens on the mailbox, the address typed for the person
    expect(byTestid("connector-wizard")?.dataset.kind).toBe("mailbox");
    expect((byTestid("mailbox-email") as HTMLInputElement | null)?.value).toBe("ada@example.com");
  });

  test("Open the client, from the sign-in of the flow, shows the card of the provider", async () => {
    // GIVEN the flow of Microsoft, whose sign-in says the client is missing
    answers.oauth_start_flow = () => {
      throw { kind: "oauth_client_not_configured", detail: "no client" };
    };
    await mountPage();
    await click("connections-open-catalogue");
    await choose("catalogue-native-microsoft");
    await click("catalogue-card-add");
    await click("add-flow-signin");
    expect(byTestid("oauth-open-settings-btn")).not.toBeNull();

    // WHEN "Open the client" is clicked
    await click("oauth-open-settings-btn");

    // THEN the window has given way, and the card of Microsoft is open
    expect(byTestid("catalogue-window")).toBeNull();
    expect(byTestid("connector-detail")).not.toBeNull();
    expect(byTestid("connection-detail-title")?.textContent).toContain("Microsoft");
  });
});
