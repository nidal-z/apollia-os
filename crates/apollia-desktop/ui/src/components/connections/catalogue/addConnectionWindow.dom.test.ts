import { describe, test, expect, vi, beforeEach, afterEach } from "vitest";
import { flushSync, mount, unmount } from "svelte";
import { locale, waitLocale } from "svelte-i18n";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => null) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

import "$lib/i18n";
import { developerMode } from "$lib/stores/developerMode";
import type { RegistryServerView } from "$lib/types";
import AddConnectionWindow from "./AddConnectionWindow.svelte";

// The dialog runs a Svelte transition, which jsdom cannot play: an animation
// that finishes at once stands in for it.
Element.prototype.animate = function (): Animation {
  const animation = {
    onfinish: null as (() => void) | null,
    cancel: () => {},
    finished: Promise.resolve(),
  };
  setTimeout(() => animation.onfinish?.(), 0);
  return animation as unknown as Animation;
};

// The automation clicks a row with `element.click()` on its anchor: the event
// starts at the anchor and only bubbles up, so the anchor has to be the element
// that chooses, not a container of it.

const NOTION: RegistryServerView = {
  name: "com.notion/mcp",
  title: "Notion",
  description: "Pages and databases",
  version: "1.0.0",
  repository_url: null,
  website_url: null,
  packages: null,
  trust_level: "verified_official",
  category: "productivity",
  enrichment: null,
  is_installed: false,
  remotes: [{ type: "streamable-http", url: "https://mcp.notion.com/mcp", headers: [] }],
};

const noop = () => {};
let target: HTMLElement;
let mounted: ReturnType<typeof mount> | null = null;
const onchoose = vi.fn((_key: string) => {});

async function settle(rounds = 10): Promise<void> {
  for (let i = 0; i < rounds; i++) {
    await new Promise((resolve) => setTimeout(resolve, 0));
    flushSync();
  }
}

beforeEach(async () => {
  locale.set("en");
  await waitLocale("en");
  developerMode.set(false);
  onchoose.mockReset();
  target = document.createElement("div");
  document.body.appendChild(target);
  mounted = mount(AddConnectionWindow as never, {
    target,
    props: {
      open: true, query: "", chosen: null, adding: false, resume: null, selection: [NOTION],
      publicEntries: [], failures: [], catalogueTier: "curated", loading: false, agents: [],
      installedNames: new Set(), installedByPackage: new Map(), onclose: noop, onchoose,
      onback: noop, onexploreall: noop, onbacktoselection: noop, onadd: noop, onmanage: noop,
      onadded: noop, ondone: noop, onsignin: noop, ontry: noop, ontryprompt: noop, onopenrule: noop,
    } as never,
  });
  await settle();
});

afterEach(() => {
  if (mounted) unmount(mounted);
  mounted = null;
  target.remove();
  document.body.innerHTML = "";
});

describe("a row of the add window is chosen by a click on its anchor", () => {
  test.each([
    ["catalogue-entry-com.notion/mcp", "entry:com.notion/mcp"],
    ["catalogue-native-google", "native:google"],
    ["catalogue-folder", "folder"],
    ["catalogue-native-mailbox", "mailbox"],
    ["catalogue-address", "address"],
  ])("%s", async (testid, key) => {
    // GIVEN the window open on its groups
    const anchor = document.querySelector<HTMLElement>(`[data-testid="${CSS.escape(testid)}"]`);
    expect(anchor).not.toBeNull();

    // WHEN the anchor itself is clicked, as the automation does
    anchor?.click();
    await settle();

    // THEN that entry is chosen
    expect(onchoose).toHaveBeenCalledTimes(1);
    expect(onchoose).toHaveBeenCalledWith(key);
  });
});
