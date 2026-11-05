import { beforeAll, describe, expect, test, vi } from "vitest";
import { render as renderToString } from "svelte/server";
import type { Component } from "svelte";
import { locale, waitLocale } from "svelte-i18n";
import "$lib/i18n";

// Stores read `localStorage` at import time; the node environment has none.
vi.hoisted(() => {
  const memory = new Map<string, string>();
  globalThis.localStorage = {
    getItem: (k: string) => memory.get(k) ?? null,
    setItem: (k: string, v: string) => void memory.set(k, v),
    removeItem: (k: string) => void memory.delete(k),
    clear: () => memory.clear(),
    key: (i: number) => Array.from(memory.keys())[i] ?? null,
    get length() {
      return memory.size;
    },
  };
});

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => null) }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
  emit: vi.fn(async () => {}),
}));

import SourceRowView from "./SourceRow.svelte";
import type { SourceRow } from "./agentSources";

function row(over: Partial<SourceRow>): SourceRow {
  return {
    id: "src-1",
    kind: "folder",
    label: "Scans",
    checked: true,
    state: "ready",
    documents: 2,
    tooLarge: 0,
    unreadable: 0,
    awaitingOcr: 0,
    notRead: 0,
    skipped: [],
    reference: "folder:scans",
    registered: true,
    ...over,
  };
}

/** The markup `component` draws with `props`, rendered on the server. */
function html<P extends Record<string, unknown>>(component: Component<P>, props: P): string {
  return renderToString(component as Component<Record<string, unknown>>, { props }).body;
}

/** The status line of the row, rendered on the server in `lang`. */
async function statusLine(over: Partial<SourceRow>, lang: "en" | "fr"): Promise<string> {
  locale.set(lang);
  await waitLocale();
  const markup = html(SourceRowView, { row: row(over) });
  const start = markup.indexOf('data-testid="agent-sheet-source-status"');
  return markup.slice(start, markup.indexOf("</p>", start));
}

beforeAll(async () => {
  locale.set("en");
  await waitLocale();
});

describe("the pages without text on a source row", () => {
  test("pages waiting for recognition and pages not read are said, in the plural", async () => {
    // GIVEN a row with three pages awaiting recognition and one page not read
    const over = { awaitingOcr: 3, notRead: 1 };

    // WHEN its status line is drawn in English and in French
    const en = await statusLine(over, "en");
    const fr = await statusLine(over, "fr");

    // THEN both counts are said, each under its anchor
    expect(en).toContain('data-testid="agent-sheet-source-awaiting-ocr"');
    expect(en).toContain('data-testid="agent-sheet-source-not-read"');
    expect(en).toContain("3 pages awaiting recognition");
    expect(en).toContain("1 page not read");
    expect(fr).toContain("3 pages en attente de reconnaissance");
    expect(fr).toContain("1 page non lue");
  });

  test("nothing is said when every page was read", async () => {
    // GIVEN a row with no page left without text
    // WHEN its status line is drawn
    const en = await statusLine({}, "en");

    // THEN neither line appears
    expect(en).not.toContain("agent-sheet-source-awaiting-ocr");
    expect(en).not.toContain("agent-sheet-source-not-read");
  });
});
