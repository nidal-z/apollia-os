import { describe, test, expect, vi } from "vitest";
import { render as renderToString } from "svelte/server";
import { locale, waitLocale } from "svelte-i18n";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => null) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(async () => null) }));

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
import fr from "$lib/i18n/fr.json";
import AddConnectionWindow from "./AddConnectionWindow.svelte";
import ConnectionCard from "../detail/ConnectionCard.svelte";
import type { Connection } from "$lib/types";

const noop = () => {};

const FOLDER: Connection = {
  id: "folder:f1",
  kind: "folder",
  label: "Clients",
  account: null,
  path: "/Users/ada/Documents/Clients",
  provenance: "selection",
  remote: false,
  official: false,
  state: "connected",
  status: "working",
  reason: null,
  checkedAt: null,
  package: null,
  tools: [
    { id: "file_read", family: "read", familySource: "builtin", prechecked: true },
    { id: "file_edit", family: "write_or_delete", familySource: "builtin", prechecked: false },
  ],
};

function text(html: string): string {
  return html
    .replace(/<!--[\s\S]*?-->/g, " ")
    .replace(/<[^>]+>/g, " ")
    .replace(/\s+/g, " ");
}

async function french(): Promise<void> {
  locale.set("fr");
  await waitLocale("fr");
}

describe("the folder in the add window", () => {
  test("the add window offers Folder under files, and the mailbox under communication", async () => {
    // GIVEN the add window, in French
    await french();

    // WHEN it is rendered
    const html = renderToString(AddConnectionWindow as never, {
      props: {
        open: true, query: "", chosen: null, selection: [], publicEntries: [], failures: [],
        catalogueTier: "curated", loading: false, agents: [], installedNames: new Set(),
        installedByPackage: new Map(), onclose: noop, onchoose: noop, onback: noop,
        onexploreall: noop, onadd: noop, onmanage: noop,
        adding: false, resume: null, onadded: noop, ondone: noop, onsignin: noop,
        ontry: noop, ontryprompt: noop, onopenrule: noop,
      } as never,
    }).body;
    const files = html.slice(html.indexOf('data-testid="catalogue-usage-files"'));
    const communication = html.slice(
      html.indexOf('data-testid="catalogue-usage-communication"'),
      html.indexOf('data-testid="catalogue-usage-documents"'),
    );

    // THEN "Dossier" sits under "Fichiers", the mailbox under "Communication"
    expect(files).toContain('data-testid="catalogue-folder"');
    expect(communication).toContain('data-testid="catalogue-native-mailbox"');
    expect(text(html)).toContain(fr.connections.usage.files);
    expect(text(html)).toContain(fr.connections.folder.name);
  });
});

describe("the folder connection", () => {
  test("the card of a folder gives its path and a way to remove it", async () => {
    // GIVEN a folder connection, in French
    await french();
    const handlers = { onupdated: noop, onremoved: noop, onreload: noop, onrepair: noop };

    // WHEN its card is rendered, and the card of a mailbox
    const folderPage = renderToString(ConnectionCard as never, {
      props: { connection: FOLDER, label: "Clients", ...handlers } as never,
    }).body;
    const mailPage = renderToString(ConnectionCard as never, {
      props: { connection: { ...FOLDER, id: "imap:ada@example.com", kind: "imap", path: undefined, account: "ada@example.com" }, label: "Mail", ...handlers } as never,
    }).body;

    // THEN the folder carries its path, the mailbox its address, and both the same Remove
    expect(text(folderPage)).toContain("/Users/ada/Documents/Clients");
    expect(text(mailPage)).toContain("ada@example.com");
    expect(folderPage).toContain('data-testid="connection-remove-btn"');
    expect(mailPage).toContain('data-testid="connection-remove-btn"');
  });
});
