import { vi, describe, test, expect, beforeAll } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue(""),
}));

import { existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { get } from "svelte/store";
import { locale, t, waitLocale } from "svelte-i18n";
import "$lib/i18n";
import en from "$lib/i18n/en.json";
import fr from "$lib/i18n/fr.json";
import { allowedGuideRoute } from "$lib/apolliaGuide/actionButtons";
import { visibleRoutes } from "$lib/apolliaGuide/guideRoutes";
import {
  SCREEN_CONTEXTS,
  SECONDARY_LEAVES,
  guideContextId,
  guideContextIds,
  guideContextKey,
  guideContextPrompt,
  guideContextRoute,
  guideDocumentationRoute,
  guideScreenView,
  type GuideContextId,
} from "$lib/apolliaGuide/screenContexts";
import {
  GUIDE_SUBSCREENS,
  ONBOARDING_GUIDE_ROUTE,
  type GuideContext,
} from "$lib/stores/guideContext";

const CATALOGUES: [string, unknown][] = [
  ["en.json", en],
  ["fr.json", fr],
];

function leafAt(catalogue: unknown, key: string): string | undefined {
  let node = catalogue;
  for (const segment of key.split(".")) {
    if (typeof node !== "object" || node === null) return undefined;
    node = (node as Record<string, unknown>)[segment];
  }
  return typeof node === "string" ? node : undefined;
}

/** The catalogue leaves a context must carry, from what the table declares. */
function leavesOf(id: GuideContextId): string[] {
  const declared = SCREEN_CONTEXTS[id];
  return [
    "text",
    "question",
    ...(declared.named ? ["name"] : []),
    ...(declared.action ? ["action.label", "action.result"] : []),
    ...SECONDARY_LEAVES.slice(0, declared.secondary.length),
  ];
}

/** The catalogue keys missing for `ids`, as `<catalogue>: <key>`. */
function missingKeys(ids: GuideContextId[], catalogues: [string, unknown][]): string[] {
  const missing: string[] = [];
  for (const [name, catalogue] of catalogues) {
    for (const id of ids) {
      for (const leaf of leavesOf(id)) {
        const key = guideContextKey(id, leaf);
        if (!leafAt(catalogue, key)?.trim()) missing.push(`${name}: ${key}`);
      }
    }
  }
  return missing;
}

async function inLanguage(language: "en" | "fr"): Promise<void> {
  locale.set(language);
  await waitLocale();
}

const AGENT_SHEET: GuideContext = {
  route: "agents",
  subscreen: "agents.fiche",
  object: { id: "agent-7", name: "Qualify requests" },
};

beforeAll(async () => {
  await inLanguage("en");
});

describe("guide contexts - one per screen the user can be on", () => {
  test("every visible route, every declared sub-screen and the first launch has its context", () => {
    // GIVEN the routes a release build shows, the sub-screens the screens
    // declare, and the first launch
    const expected = [...visibleRoutes(true), ...GUIDE_SUBSCREENS, ONBOARDING_GUIDE_ROUTE];

    // WHEN the contexts of the table are listed
    const declared = guideContextIds();

    // THEN they are the same set: none missing, none for a screen that is gone
    expect([...declared].sort()).toEqual([...expected].sort());
  });

  test("each context has its texts in English and in French", () => {
    // GIVEN the two catalogues
    // WHEN the keys of every context are looked up in both
    const missing = missingKeys(guideContextIds(), CATALOGUES);

    // THEN none is missing or blank
    expect(missing).toEqual([]);
  });

  test("a context whose key is absent from one language is reported", () => {
    // GIVEN a French catalogue that lost the text of the agent sheet
    const withoutSheet = structuredClone(fr) as { guide: { context: { agents: { fiche: { text?: string } } } } };
    delete withoutSheet.guide.context.agents.fiche.text;

    // WHEN the keys are looked up
    const missing = missingKeys(guideContextIds(), [
      ["en.json", en],
      ["fr.json", withoutSheet],
    ]);

    // THEN the missing key is named, with its catalogue
    expect(missing).toEqual(["fr.json: guide.context.agents.fiche.text"]);
  });

  test("no text names a page that does not exist or the retired agent contract", () => {
    // GIVEN every text of every context, in both languages
    const retired = /\b(triggers?|pipelines?)\b|manifest\(\)|run\(\)/i;

    // WHEN each is searched for the pages and the contract that are gone
    const faulty: string[] = [];
    for (const [name, catalogue] of CATALOGUES) {
      for (const id of guideContextIds()) {
        for (const leaf of leavesOf(id)) {
          const value = leafAt(catalogue, guideContextKey(id, leaf)) ?? "";
          if (retired.test(value)) faulty.push(`${name}: ${guideContextKey(id, leaf)}`);
        }
      }
    }

    // THEN none carries them
    expect(faulty).toEqual([]);
    expect(retired.test("You are on the Triggers page.")).toBe(true);
  });
});

describe("guide contexts - the actions lead to real screens", () => {
  test("every action opens a visible route, other than the screen it is offered on", () => {
    // GIVEN the actions each context declares
    for (const id of guideContextIds()) {
      const { action, secondary } = SCREEN_CONTEXTS[id];
      const targets = [...(action ? [action] : []), ...secondary];

      // WHEN each target is put through the allowlist, developer mode on
      // THEN it is let through, and it is not the screen the user is on
      for (const target of targets) {
        expect(allowedGuideRoute(`/${target}`, true), `${id} -> ${target}`).toBe(target);
        expect(target, `${id} offers its own screen`).not.toBe(guideContextRoute(id));
      }
      expect(new Set(targets).size, `${id} offers a screen twice`).toBe(targets.length);
    }
  });

  test("every context but the first launch offers an action", () => {
    // GIVEN the contexts
    // WHEN those without an offered action are listed
    const without = guideContextIds().filter((id) => SCREEN_CONTEXTS[id].action === null);

    // THEN only the first launch is, its window covering the other screens
    expect(without).toEqual([ONBOARDING_GUIDE_ROUTE]);
  });

  test("with the developer mode off, an action towards a developer screen is not offered", () => {
    // GIVEN a context whose lesser action opens the Memory screen
    const context: GuideContext = { route: "agents", subscreen: null, object: null };
    const original = SCREEN_CONTEXTS.agents;
    SCREEN_CONTEXTS.agents = { ...original, secondary: ["memory", "inbox"] };

    // WHEN the view is built with the mode off, then on
    const off = guideScreenView(context, get(t), false);
    const on = guideScreenView(context, get(t), true);
    SCREEN_CONTEXTS.agents = original;

    // THEN the Memory action is only offered with the mode on
    expect(off?.secondary.map((action) => action.route)).toEqual(["inbox"]);
    expect(on?.secondary.map((action) => action.route)).toEqual(["memory", "inbox"]);
  });
});

describe("guide contexts - what the tab shows", () => {
  test("on the sheet of an agent, the text of the sheet is shown in French, then in English", async () => {
    // GIVEN the sheet of an agent open, the interface in French
    await inLanguage("fr");
    const french = guideScreenView(AGENT_SHEET, get(t), false);

    // WHEN the language is switched to English
    await inLanguage("en");
    const english = guideScreenView(AGENT_SHEET, get(t), false);

    // THEN each view carries the text of the sheet in its language, naming
    // the agent, under the same context
    expect(french?.id).toBe("agents.fiche");
    const key = "guide.context.agents.fiche.text";
    expect(french?.text).toBe(leafAt(fr, key)?.replace("{name}", "Qualify requests"));
    expect(french?.trail).toEqual(["Agents", "Qualify requests"]);
    expect(english?.text).toBe(
      "You are reading the sheet of Qualify requests. It says what this agent does, which model answers for it, what it may do and what it already did.",
    );
    expect(french?.text).not.toBe(english?.text);
    expect(english?.title).toBe("Qualify requests");
  });

  test("the action offered on the sheet opens the screen it announces", () => {
    // GIVEN the sheet of an agent open
    // WHEN its view is built
    const view = guideScreenView(AGENT_SHEET, get(t), false);

    // THEN the offered action hands a task in the Chat, and the lesser ones
    // open Permissions and the Activity log
    expect(view?.action).toEqual({
      label: "Hand it a task",
      result: "Give it a task in the Chat; you follow its steps there.",
      route: "chat",
    });
    expect(view?.secondary.map((action) => [action.label, action.route])).toEqual([
      ["Review its permissions", "authorizations"],
      ["See its activity in the Activity log", "journal"],
    ]);
  });

  test("a sub-screen with no object open carries its own name", () => {
    // GIVEN the sheet declared without the agent it shows
    const context: GuideContext = { route: "agents", subscreen: "agents.fiche", object: null };

    // WHEN its view is built
    const view = guideScreenView(context, get(t), false);

    // THEN the trail ends on the name of the sub-screen, and the text falls
    // back on a plain word for the agent
    expect(view?.trail).toEqual(["Agents", "Sheet of an agent"]);
    expect(view?.text).toContain("the sheet of this agent.");
  });

  test("the first launch has its own context, without an action", () => {
    // GIVEN the first-launch window open over the Chat
    const context: GuideContext = { route: ONBOARDING_GUIDE_ROUTE, subscreen: null, object: null };

    // WHEN its view is built
    const view = guideScreenView(context, get(t), false);

    // THEN it is named, described, and offers nothing to open
    expect(view?.id).toBe("onboarding");
    expect(view?.trail).toEqual(["Getting acquainted"]);
    expect(view?.action).toBeNull();
    expect(view?.secondary).toEqual([]);
  });

  test("a route outside the visible ones has no context", () => {
    // GIVEN the design reference of a development build, and a removed screen
    // WHEN their context is asked for
    // THEN there is none, and nothing is said to the guide about them
    for (const route of ["design", "design-motion", "triggers"]) {
      const context: GuideContext = { route, subscreen: null, object: null };
      expect(guideContextId(context)).toBeNull();
      expect(guideScreenView(context, get(t), true)).toBeNull();
      expect(guideContextPrompt(context, get(t))).toBe("");
    }
  });
});

describe("guide contexts - the documentation page of each", () => {
  test("every context has its page on the documentation site, in both languages", () => {
    // GIVEN the documentation site of this repository
    const site = fileURLToPath(new URL("../../../../../../docs/site/", import.meta.url));
    const roots = ["docs", "i18n/fr/docusaurus-plugin-content-docs/current"];

    // WHEN the page of each context is looked for under both trees
    const absent: string[] = [];
    for (const id of guideContextIds()) {
      const route = guideDocumentationRoute(id);
      for (const root of roots) {
        if (!existsSync(`${site}${root}${route}.md`)) absent.push(`${root}${route}.md`);
      }
    }

    // THEN each is there, and no two contexts share a page
    expect(absent).toEqual([]);
    const pages = guideContextIds().map(guideDocumentationRoute);
    expect(new Set(pages).size).toBe(pages.length);
  });
});
