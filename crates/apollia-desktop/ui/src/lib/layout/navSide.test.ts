import { describe, test, expect, vi, beforeAll } from "vitest";
import { render } from "svelte/server";
import { locale, waitLocale } from "svelte-i18n";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => null) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

// The layout and appearance stores read the browser storage at import; node has none.
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
import { navSide } from "./navSide";
import Sidebar from "$lib/components/app/Sidebar.svelte";
import DetailSidebar from "$lib/components/app/DetailSidebar.svelte";
import { CHAT_APPEARANCE_DEFAULTS, chatAppearance, setChatAppearance } from "$lib/stores/chatAppearance";
import { detailSidebar } from "$lib/stores/detailSidebar";

beforeAll(async () => {
  locale.set("en");
  await waitLocale("en");
});

describe("navigation position - the sides of the shell", () => {
  test("on the left, the default, the shell reads navigation, screen, detail sidebar", () => {
    // GIVEN the default position
    // WHEN the sides are resolved
    const side = navSide(CHAT_APPEARANCE_DEFAULTS.navPosition);

    // THEN the bar is on the left, the detail sidebar on the right, and both
    // rows keep the document order
    expect(side.nav).toBe("left");
    expect(side.detail).toBe("right");
    expect(side.shellRow).toBe("flex-row");
    expect(side.contentRow).toBe("flex-row");
    expect(side.railBorder).toBe("border-r");
    expect(side.detailBorder).toBe("border-l");
  });

  test("on the right, the bar crosses the shell and the detail sidebar goes left", () => {
    // GIVEN the navigation set to the right
    // WHEN the sides are resolved
    const side = navSide("right");

    // THEN both rows are reversed, each column borders the content on its
    // inner edge, and the tooltips and the drawer open toward the content
    expect(side.nav).toBe("right");
    expect(side.detail).toBe("left");
    expect(side.shellRow).toBe("flex-row-reverse");
    expect(side.contentRow).toBe("flex-row-reverse");
    expect(side.railBorder).toBe("border-l");
    expect(side.detailBorder).toBe("border-r");
    expect(side.railTooltip).toContain("right-full");
    expect(side.drawerEdge).toContain("right-0");
    expect(side.drawerFlyX).toBeGreaterThan(0);
  });

  test("the detail sidebar is never on the side of the navigation", () => {
    // GIVEN both positions
    // WHEN the sides are resolved
    const sides = (["left", "right"] as const).map(navSide);

    // THEN the two columns are always opposite
    for (const side of sides) expect(side.detail).not.toBe(side.nav);
  });
});

describe("navigation position - what the columns render", () => {
  test("the rail and the detail sidebar follow the setting", () => {
    // GIVEN the detail sidebar open and the navigation on the left
    detailSidebar.openGuide();
    setChatAppearance({ navPosition: "left" });
    const railLeft = render(Sidebar).body;
    const detailLeft = render(DetailSidebar).body;

    // WHEN the navigation is set to the right
    setChatAppearance({ navPosition: "right" });
    const railRight = render(Sidebar).body;
    const detailRight = render(DetailSidebar).body;

    // THEN the rail names its new side and the detail sidebar the opposite one
    expect(railLeft).toContain('data-nav-position="left"');
    expect(detailLeft).toContain('data-side="right"');
    expect(railRight).toContain('data-nav-position="right"');
    expect(railRight).toContain("border-l");
    expect(detailRight).toContain('data-side="left"');
    expect(detailRight).toContain("border-r");
    chatAppearance.set({ ...CHAT_APPEARANCE_DEFAULTS });
    detailSidebar.close();
  });
});
