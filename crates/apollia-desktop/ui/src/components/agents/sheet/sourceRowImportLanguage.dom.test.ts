import { afterEach, beforeAll, describe, expect, test, vi } from "vitest";
import { flushSync, mount, unmount } from "svelte";
import { get } from "svelte/store";
import { locale, waitLocale } from "svelte-i18n";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => null) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

import "$lib/i18n";
import { currentRoute, navigateTo } from "$lib/stores/navigation";
import SourceRowView from "./SourceRow.svelte";
import type { SourceRow } from "./agentSources";

// A source row of the agent sheet mounted in the DOM: the link to import a
// reading language appears with pages waiting for recognition, and leads to
// the AI screen.

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

let mounted: Record<string, unknown> | null = null;

function show(over: Partial<SourceRow>): HTMLElement {
  const target = document.createElement("ul");
  document.body.appendChild(target);
  mounted = mount(SourceRowView, { target, props: { row: row(over) } });
  flushSync();
  return target;
}

beforeAll(async () => {
  locale.set("en");
  await waitLocale();
});

afterEach(() => {
  if (mounted) unmount(mounted);
  mounted = null;
  document.body.innerHTML = "";
});

describe("the link to import a reading language on a source row", () => {
  test("a row with pages awaiting recognition offers it, and it opens the AI screen", () => {
    // GIVEN a row with two pages awaiting recognition, and the agents screen shown
    navigateTo("agents");
    const target = show({ awaitingOcr: 2 });

    // WHEN its link is clicked
    const link = target.querySelector<HTMLButtonElement>(
      '[data-testid="agent-sheet-source-import-language"]',
    );
    expect(link?.textContent?.trim()).toBe("Import the language");
    link?.click();
    flushSync();

    // THEN the AI screen is shown
    expect(get(currentRoute)).toBe("llm");
  });

  test("a row with every page read does not offer it", () => {
    // GIVEN a row with no page awaiting recognition, one of them not read
    // WHEN it is drawn
    const target = show({ awaitingOcr: 0, notRead: 1 });

    // THEN no link to import a language is there
    expect(target.querySelector('[data-testid="agent-sheet-source-import-language"]')).toBeNull();
  });
});
