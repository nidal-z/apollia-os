import { describe, test, expect, vi, beforeAll, afterEach } from "vitest";
import { render } from "svelte/server";
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

// The tooltip needs its provider at the root; a plain wrapper is enough here.
vi.mock("$lib/components/ui/tooltip", () => ({
  Tooltip: (
    renderer: { push: (html: string) => void },
    props: { content: string; children: (renderer: unknown) => void },
  ) => {
    renderer.push(`<span data-tooltip="${props.content}">`);
    props.children(renderer);
    renderer.push("</span>");
  },
}));

import ChatPaneTabs, { type PaneTab } from "./ChatPaneTabs.svelte";
import AppearanceChatSettings from "../settings/AppearanceChatSettings.svelte";
import { CHAT_APPEARANCE_DEFAULTS, chatAppearance } from "$lib/stores/chatAppearance";

import fr from "$lib/i18n/fr.json";
import en from "$lib/i18n/en.json";

const TWO: PaneTab[] = [
  { id: "main", title: "March invoices" },
  { id: "pane-2", title: "Supplier follow-up" },
];
const FOUR: PaneTab[] = [
  ...TWO,
  { id: "pane-3", title: "Quarterly review of the open purchase orders" },
  { id: "pane-4", title: "Draft of the answer to the audit questions" },
];

/** The opening tag of the row of tabs. */
function railTag(html: string): string {
  return /<div[^>]*data-testid="chat-pane-tabs"[^>]*>/.exec(html)?.[0] ?? "";
}

function renderTabs(tabs: PaneTab[], showScrollbar: boolean, activeId = "main"): string {
  return render(ChatPaneTabs, {
    props: { tabs, activeId, showScrollbar, onactivate: () => {} },
  }).body;
}

beforeAll(async () => {
  locale.set("en");
  await waitLocale();
});

afterEach(() => {
  chatAppearance.set({ ...CHAT_APPEARANCE_DEFAULTS });
});

describe("the row of tabs of the split chat", () => {
  test("setting off, a row that overflows scrolls and shows no bar", () => {
    // GIVEN four tabs, more than a narrow window holds, and the setting off
    // WHEN the row renders
    const rail = railTag(renderTabs(FOUR, false));

    // THEN it scrolls sideways and its bar is removed
    expect(rail).toContain("overflow-x-auto");
    expect(rail).toContain("scrollbar-none");
    expect(rail).toContain('data-scrollbar="hidden"');
  });

  test("setting on, a row that overflows shows its bar", () => {
    // GIVEN the same four tabs and the setting on
    // WHEN the row renders
    const rail = railTag(renderTabs(FOUR, true));

    // THEN it scrolls sideways and nothing removes its bar
    expect(rail).toContain("overflow-x-auto");
    expect(rail).not.toContain("scrollbar-none");
    expect(rail).toContain('data-scrollbar="shown"');
  });

  test("a row that holds its tabs never forces a bar, under either value", () => {
    // GIVEN two tabs, which fit
    // WHEN the row renders with the setting on, then off
    const rails = [railTag(renderTabs(TWO, true)), railTag(renderTabs(TWO, false))];

    // THEN the overflow stays automatic: a bar exists only on overflow
    for (const rail of rails) {
      expect(rail).toContain("overflow-x-auto");
      expect(rail).not.toMatch(/overflow-(x-)?scroll/);
    }
  });

  test("the active tab is the one the keyboard enters the row by", () => {
    // GIVEN four tabs, the third active
    // WHEN the row renders
    const html = renderTabs(FOUR, false, "pane-3");
    const tabs = html.match(/<button[^>]*data-testid="chat-pane-tab"[^>]*>/g) ?? [];

    // THEN it alone is selected and in the tab order; the arrows reach the others
    expect(tabs).toHaveLength(4);
    const selected = tabs.filter((tab) => tab.includes('aria-selected="true"'));
    expect(selected).toHaveLength(1);
    expect(selected[0]).toContain('data-pane-tab="pane-3"');
    expect(selected[0]).toContain('tabindex="0"');
    expect(tabs.filter((tab) => tab.includes('tabindex="-1"'))).toHaveLength(3);
  });
});

describe("Settings > Appearance, the tab scroll bar", () => {
  test("the row sits in Chat style, right after the view mode, off by default", () => {
    // GIVEN the defaults
    // WHEN the "Chat style" block renders
    const html = render(AppearanceChatSettings).body;
    const viewMode = html.indexOf('data-testid="appearance-view-mode"');
    const scrollbar = html.indexOf('data-testid="appearance-tab-scrollbar"');
    const fontSize = html.indexOf('data-testid="appearance-font-size"');
    const toggle =
      /<button[^>]*data-testid="appearance-tab-scrollbar-toggle"[^>]*>/.exec(html)?.[0] ?? "";

    // THEN the row comes after the view mode and before the text size, and its
    // switch is off
    expect(viewMode).toBeGreaterThan(-1);
    expect(scrollbar).toBeGreaterThan(viewMode);
    expect(fontSize).toBeGreaterThan(scrollbar);
    expect(toggle).toContain('aria-checked="false"');
    expect(CHAT_APPEARANCE_DEFAULTS.tabScrollbar).toBe(false);
  });

  test("the switch reads the stored choice", () => {
    // GIVEN a reader who turned the bar on
    chatAppearance.set({ ...CHAT_APPEARANCE_DEFAULTS, tabScrollbar: true });

    // WHEN the block renders
    const html = render(AppearanceChatSettings).body;
    const toggle =
      /<button[^>]*data-testid="appearance-tab-scrollbar-toggle"[^>]*>/.exec(html)?.[0] ?? "";

    // THEN the switch is on
    expect(toggle).toContain('aria-checked="true"');
  });

  test("the explanation names the split view, in both languages", () => {
    // GIVEN the "Chat style" block in English
    // WHEN it renders
    const html = render(AppearanceChatSettings).body;

    // THEN the label and the explanation are shown, and each language names
    // the split view in its own words
    expect(html).toContain(en.settings.appearance.tab_scrollbar_label);
    expect(html).toContain(en.settings.appearance.tab_scrollbar_hint);
    expect(en.settings.appearance.tab_scrollbar_hint).toContain("split view");
    expect(fr.settings.appearance.tab_scrollbar_hint).toMatch(/divis/);
    expect(fr.settings.appearance.tab_scrollbar_label).not.toBe(
      en.settings.appearance.tab_scrollbar_label,
    );
    expect(fr.settings.appearance.tab_scrollbar_hint).not.toBe(
      en.settings.appearance.tab_scrollbar_hint,
    );
  });
});
