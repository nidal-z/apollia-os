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
import FolderConnectionDialog from "./FolderConnectionDialog.svelte";
import CatalogueSheet from "./CatalogueSheet.svelte";
import ConnectionSummaryDetail from "../detail/ConnectionSummaryDetail.svelte";
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

describe("the folder in the selection", () => {
  test("the add sheet offers Folder first, built in", async () => {
    // GIVEN the add sheet, in French
    await french();

    // WHEN it is rendered
    const html = renderToString(CatalogueSheet as never, {
      props: {
        open: true, registry: [], agents: [], installedNames: new Set(), installedByPackage: new Map(),
        registryLoading: false, catalogueTier: "curated", onclose: noop, onconnect: noop,
        onmanage: noop, oninstalled: noop, onaddnative: noop, onaddfolder: noop,
      } as never,
    }).body;

    // THEN "Dossier" comes before the native accounts, with the "Intégrée" badge
    expect(html).toContain('data-testid="catalogue-folder"');
    expect(html.indexOf("catalogue-folder")).toBeLessThan(html.indexOf("catalogue-native-google"));
    expect(text(html)).toContain(fr.connections.folder.name);
    expect(text(html)).toContain(fr.connections.badge_builtin);
  });
});

describe("the folder added", () => {
  test("the dialog names the folder, its path and what agents may do there", async () => {
    // GIVEN a folder just added, in French
    await french();

    // WHEN the dialog is rendered
    const html = renderToString(FolderConnectionDialog as never, {
      props: { open: true, outcome: { kind: "added", connection: FOLDER }, onclose: noop, onchooseanother: noop, onopenrule: noop } as never,
    }).body;
    const words = text(html);

    // THEN the title, the readable path, both abilities and both buttons are there
    expect(words).toContain(fr.connections.folder.added_title.replace("{name}", "Clients"));
    expect(words).toContain("Documents › Clients");
    expect(words).toContain(fr.connections.folder.ability_read);
    expect(words).toContain(fr.connections.folder.ability_write);
    expect(html).toContain('data-testid="folder-connection-choose-another"');
    expect(html).toContain('data-testid="folder-connection-done"');
  });

  test("a refusal shows the one refusal box", async () => {
    // GIVEN a folder refused by the organisation
    await french();
    const outcome = {
      kind: "refused",
      refusal: { code: "policy_unreadable", by: "organisation", reason: "the connection policy could not be read" },
    };

    // WHEN the dialog is rendered
    const html = renderToString(FolderConnectionDialog as never, {
      props: { open: true, outcome, onclose: noop, onchooseanother: noop, onopenrule: noop } as never,
    }).body;

    // THEN the refusal box is shown, not the folder
    expect(html).toContain('data-testid="connection-refusal"');
    expect(html).not.toContain('data-testid="folder-connection-added"');
  });

  test("the page of a folder gives its path and a way to remove it", async () => {
    // GIVEN a folder connection, in French
    await french();

    // WHEN its page is rendered, and the page of a mailbox
    const folderPage = renderToString(ConnectionSummaryDetail as never, {
      props: { connection: FOLDER, label: "Clients" } as never,
    }).body;
    const mailPage = renderToString(ConnectionSummaryDetail as never, {
      props: { connection: { ...FOLDER, id: "imap:ada@example.com", kind: "imap", path: undefined, account: "ada@example.com" }, label: "Mail" } as never,
    }).body;

    // THEN only the folder carries its path and the remove button
    expect(text(folderPage)).toContain("/Users/ada/Documents/Clients");
    expect(folderPage).toContain('data-testid="connection-folder-remove"');
    expect(mailPage).not.toContain('data-testid="connection-folder-remove"');
  });
});
