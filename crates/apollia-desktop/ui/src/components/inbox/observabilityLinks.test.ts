import { describe, test, expect, vi } from "vitest";
import { render as renderToString } from "svelte/server";
import { locale, waitLocale } from "svelte-i18n";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => null) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

// The theme store reads the browser storage at import; node has none.
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
import ActivityRow from "./ActivityRow.svelte";
import InboxActivityTab from "./InboxActivityTab.svelte";
import { developerMode } from "$lib/stores/developerMode";
import type { NotificationLogEntry } from "$lib/types";

// Observability is a developer screen: the two links that lead to it from the
// activity tab are drawn only with the developer mode on. Rendered through
// Svelte's server renderer, the only renderer the node environment offers.

const ENTRY = {
  id: 1,
  event_name: "task.failed",
  sent_at: "2026-10-03T10:00:00Z",
  task_id: "task-1",
  error: null,
} as unknown as NotificationLogEntry;

async function inEnglish(on: boolean): Promise<void> {
  locale.set("en");
  await waitLocale("en");
  developerMode.set(on);
}

function emptyTab(): string {
  return renderToString(InboxActivityTab, {
    props: {
      entries: [],
      loading: false,
      error: null,
      filter: "all",
      onFilterChange: () => {},
      onRetry: () => {},
    },
  }).body;
}

describe("activity row - the link to the logs", () => {
  test("with the developer mode off, the row has no link to Observability", async () => {
    // GIVEN the developer mode off
    await inEnglish(false);

    // WHEN an activity row is rendered
    const html = renderToString(ActivityRow, { props: { entry: ENTRY } }).body;

    // THEN the row is there without its "View logs" button
    expect(html).toContain('data-testid="activity-row"');
    expect(html).not.toContain('data-testid="activity-row-see-logs"');
    expect(html).not.toContain("View logs");
  });

  test("with the developer mode on, the row links to the logs", async () => {
    // GIVEN the developer mode on
    await inEnglish(true);

    // WHEN the same row is rendered
    const html = renderToString(ActivityRow, { props: { entry: ENTRY } }).body;

    // THEN the "View logs" button is drawn
    expect(html).toContain('data-testid="activity-row-see-logs"');
    expect(html).toContain("View logs");
  });
});

describe("activity tab - the empty state", () => {
  test("with the developer mode off, the empty state has no link to Observability", async () => {
    // GIVEN the developer mode off
    await inEnglish(false);

    // WHEN the tab renders an empty list
    const html = emptyTab();

    // THEN the empty state is shown without its call to action
    expect(html).toContain("All clear - no recent incidents.");
    expect(html).not.toContain('data-testid="inbox-activity-empty-cta"');
  });

  test("with the developer mode on, the empty state opens Observability", async () => {
    // GIVEN the developer mode on
    await inEnglish(true);

    // WHEN the tab renders an empty list
    const html = emptyTab();

    // THEN the call to action is drawn
    expect(html).toContain('data-testid="inbox-activity-empty-cta"');
    expect(html).toContain("Open Observability");
  });
});
