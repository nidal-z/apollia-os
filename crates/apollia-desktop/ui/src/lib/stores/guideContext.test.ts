import { vi, describe, test, expect, beforeAll, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue(""),
}));

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { get } from "svelte/store";
import { locale, waitLocale } from "svelte-i18n";
import "$lib/i18n";
import { invoke } from "@tauri-apps/api/core";
import {
  GUIDE_SUBSCREENS,
  ONBOARDING_GUIDE_ROUTE,
  declareGuideSubscreen,
  guideContext,
  guideContextKey,
  resetGuideSubscreen,
} from "$lib/stores/guideContext";
import { companionStore } from "$lib/stores/companion";
import { currentRoute } from "$lib/stores/navigation";
import { onboardingModalOpen } from "$lib/stores/onboarding";

const mockedInvoke = vi.mocked(invoke);

function source(relative: string): string {
  return readFileSync(fileURLToPath(new URL(relative, import.meta.url)), "utf8");
}

beforeAll(async () => {
  locale.set("en");
  await waitLocale();
});

beforeEach(() => {
  resetGuideSubscreen();
  onboardingModalOpen.set(false);
  companionStore.reset();
  vi.clearAllMocks();
  currentRoute.set("agents");
});

describe("guideContext - route and sub-screen", () => {
  test("on a screen with no declaration, the context is the route alone", () => {
    // GIVEN the Agents screen with no sub-screen declared
    // WHEN reading the context
    const context = get(guideContext);

    // THEN only the route is set
    expect(context).toEqual({ route: "agents", subscreen: null, object: null });
  });

  test("a screen that declares a sub-screen changes the context sent to the guide", async () => {
    // GIVEN an active guide session on the Agents screen
    mockedInvoke.mockImplementation((cmd: string) => {
      if (cmd === "create_companion_session") return Promise.resolve({ session_id: "s-1" });
      return Promise.resolve(null);
    });
    await companionStore.createSession("agents");
    const before = get(guideContext);

    // WHEN the screen declares the sheet of one agent, and the context is handed on
    declareGuideSubscreen("agents.fiche", { id: "a-1", name: "Web watch" });
    const after = get(guideContext);
    await companionStore.updateContext(after);

    // THEN the context changed, and the session's instructions name both
    expect(guideContextKey(after)).not.toBe(guideContextKey(before));
    expect(after).toEqual({
      route: "agents",
      subscreen: "agents.fiche",
      object: { id: "a-1", name: "Web watch" },
    });
    const call = mockedInvoke.mock.calls.find(([cmd]) => cmd === "update_chat_session");
    const prompt = (call?.[1] as { update: { system_prompt: string } }).update.system_prompt;
    expect(prompt).toContain("agents.fiche");
    expect(prompt).toContain("Web watch");
  });

  test("withdrawing the declaration returns to the screen itself", () => {
    // GIVEN a declared sub-screen
    const withdraw = declareGuideSubscreen("agents.fiche", { id: "a-1", name: "Web watch" });

    // WHEN the record closes
    withdraw();

    // THEN the context is the route alone
    expect(get(guideContext).subscreen).toBeNull();
    expect(get(guideContext).object).toBeNull();
  });

  test("a stale cleanup leaves a newer declaration in place", () => {
    // GIVEN one agent opened, then another
    const first = declareGuideSubscreen("agents.fiche", { id: "a-1", name: "One" });
    declareGuideSubscreen("agents.fiche", { id: "a-2", name: "Two" });

    // WHEN the first declaration's cleanup runs late
    first();

    // THEN the second agent is still the one the guide sees
    expect(get(guideContext).object).toEqual({ id: "a-2", name: "Two" });
  });

  test("the Agents screen declares its agent sheet with the open agent", () => {
    // GIVEN the Agents screen source
    const agents = source("../../routes/Agents.svelte");

    // WHEN looking for its declaration
    // THEN it declares a sub-screen of the list, with the agent's id and name
    expect(GUIDE_SUBSCREENS).toContain("agents.fiche");
    expect(agents).toContain('declareGuideSubscreen("agents.fiche", {');
    expect(agents).toContain("name: selected.name");
  });
});

describe("guideContext - onboarding", () => {
  test("while the onboarding modal is open, the context is onboarding", () => {
    // GIVEN a sub-screen declared on the screen behind the modal
    declareGuideSubscreen("agents.fiche", { id: "a-1", name: "Web watch" });

    // WHEN the onboarding modal opens
    onboardingModalOpen.set(true);

    // THEN the guide is told it is in the onboarding, nothing else
    expect(get(guideContext)).toEqual({
      route: ONBOARDING_GUIDE_ROUTE,
      subscreen: null,
      object: null,
    });
    expect(ONBOARDING_GUIDE_ROUTE).toBe("onboarding");
  });

  test("the top bar and the detail sidebar rise above the onboarding veil", () => {
    // GIVEN the stacking tokens and the three surfaces
    const css = source("../../app.css");
    const veil = Number(/--z-onboarding:\s*(\d+);/.exec(css)?.[1]);
    const above = Number(/--z-onboarding-above:\s*(\d+);/.exec(css)?.[1]);
    const modal = source("../../components/onboarding/OnboardingModal.svelte");
    const topbar = source("../components/app/Topbar.svelte");
    const sidebar = source("../components/app/DetailSidebar.svelte");

    // WHEN comparing the rungs and how each surface reads them
    const raise = "style:z-index={$onboardingModalOpen ? zLayer.onboardingAbove : undefined}";

    // THEN the help icon and the guide stay reachable over the modal
    expect(above).toBeGreaterThan(veil);
    expect(modal).toContain("z-index: var(--z-onboarding);");
    expect(topbar).toContain(raise);
    expect(sidebar).toContain(raise);
  });
});
