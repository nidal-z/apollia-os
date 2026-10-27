import { describe, test, expect, vi, beforeEach, afterEach } from "vitest";
import { flushSync, mount, unmount } from "svelte";
import { locale, waitLocale } from "svelte-i18n";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => null) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

import "$lib/i18n";
import type { ToolStatusDto } from "$lib/stores/toolGovernance";
import ToolConfigDrawer from "./ToolConfigDrawer.svelte";

// The sheet runs a Svelte transition, which jsdom cannot play: an animation
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

// The automation clicks with `element.click()` on an anchor: the event starts
// at the anchor and only bubbles up, so the anchor has to be the element that
// chooses (or a label of it), not the group around the choices.

const WEB_SEARCH: ToolStatusDto = {
  name: "web_search",
  enabled: true,
  config: null,
  credential_keys: [],
  active_backend: null,
};

let target: HTMLElement;
let mounted: ReturnType<typeof mount> | null = null;

async function settle(rounds = 10): Promise<void> {
  for (let i = 0; i < rounds; i++) {
    await new Promise((resolve) => setTimeout(resolve, 0));
    flushSync();
  }
}

function radio(value: string): HTMLElement | null {
  return document.querySelector<HTMLElement>(`[data-testid="web-search-backend-${value}"] [role="radio"]`);
}

beforeEach(async () => {
  locale.set("en");
  await waitLocale("en");
  target = document.createElement("div");
  document.body.appendChild(target);
  mounted = mount(ToolConfigDrawer, {
    target,
    props: { open: true, tool: WEB_SEARCH, title: "Web search", onclose: () => {} },
  });
  await settle();
});

afterEach(() => {
  if (mounted) unmount(mounted);
  mounted = null;
  target.remove();
  document.body.innerHTML = "";
});

describe("a web search backend is chosen by a click on its anchor", () => {
  test.each(["duckduckgo", "brave", "auto"])("%s", async (value) => {
    // GIVEN the drawer open on web search, a backend other than the one to choose set first
    const other = value === "auto" ? "brave" : "auto";
    document.querySelector<HTMLElement>(`[data-testid="web-search-backend-${other}"]`)?.click();
    await settle();
    expect(radio(value)?.getAttribute("aria-checked")).toBe("false");

    // WHEN the anchor itself is clicked, as the automation does
    const anchor = document.querySelector<HTMLElement>(`[data-testid="web-search-backend-${value}"]`);
    expect(anchor).not.toBeNull();
    anchor?.click();
    await settle();

    // THEN that backend is the chosen one
    expect(radio(value)?.getAttribute("aria-checked")).toBe("true");
    expect(radio(other)?.getAttribute("aria-checked")).toBe("false");
  });

  test("a click on the group chooses nothing", async () => {
    // GIVEN the drawer open on web search, on the default backend
    expect(radio("auto")?.getAttribute("aria-checked")).toBe("true");

    // WHEN the group around the choices is clicked
    document.querySelector<HTMLElement>(`[data-testid="web-search-backend-group"]`)?.click();
    await settle();

    // THEN the backend has not changed
    expect(radio("auto")?.getAttribute("aria-checked")).toBe("true");
    expect(radio("duckduckgo")?.getAttribute("aria-checked")).toBe("false");
  });
});
