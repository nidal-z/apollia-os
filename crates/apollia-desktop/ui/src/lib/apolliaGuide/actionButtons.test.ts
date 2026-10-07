import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue(""),
}));

import {
  allowedGuideRoute,
  sanitizeActionButtons,
  validateActionButton,
  type RawActionButton,
} from "./actionButtons";
import { visibleRoutes } from "./guideRoutes";
import { DEVELOPER_NAV, visibleSidebarRoutes } from "$lib/components/app/Sidebar.svelte";
import { routeMeta } from "$lib/navigation/routeMeta";
import { developerMode } from "$lib/stores/developerMode";

beforeEach(() => {
  developerMode.set(false);
});

/** Every path the allowlist lets through, probed over every name of `Route`. */
function allowedPaths(developer: boolean): string[] {
  return Object.keys(routeMeta)
    .map((route) => `/${route}`)
    .filter((path) => allowedGuideRoute(path, developer) !== null);
}

describe("apolliaGuide - the one allowlist", () => {
  it("covers exactly the visible routes, developer mode on", () => {
    // GIVEN every route of the application
    // WHEN each is put through the allowlist with the developer mode on
    const allowed = allowedPaths(true);

    // THEN what passes is every screen a release build shows, and nothing
    // else: the design reference of a development build stays out
    expect(allowed.sort()).toEqual(
      [
        "agents",
        "authorizations",
        "automations",
        "batch",
        "batches",
        "chat",
        "developer",
        "inbox",
        "integrations",
        "journal",
        "library",
        "llm",
        "memory",
        "observability",
        "policy",
        "settings",
        "transcriptions",
      ].map((route) => `/${route}`),
    );
    expect(allowed).toEqual(visibleRoutes(true).map((route) => `/${route}`).sort());
  });

  it("leaves the developer screens out with the developer mode off", () => {
    // GIVEN the developer mode off
    // WHEN the routes are put through the allowlist
    const allowed = allowedPaths(false);

    // THEN the developer screen and the developer group are refused, and
    // every other visible route still passes
    const developer = ["/developer", ...DEVELOPER_NAV.map((entry) => `/${entry.route}`)];
    for (const path of developer) expect(allowed).not.toContain(path);
    expect(allowed.length).toBe(allowedPaths(true).length - developer.length);
  });

  it("lets through every route the sidebar lists, in either mode", () => {
    // GIVEN the routes the sidebar lists with the mode off, then on
    for (const developer of [false, true]) {
      // WHEN each is put through the allowlist in the same mode
      // THEN it opens its own screen
      for (const route of visibleSidebarRoutes(developer)) {
        expect(allowedGuideRoute(`/${route}`, developer)).toBe(route);
      }
    }
  });

  it("refuses the paths of screens that do not exist", () => {
    // GIVEN the paths the former lists carried for pages that are gone, and
    // a name outside the routes
    const gone = ["triggers", "pipelines", "onboarding", "tasks", "design", "admin/destroy"];

    // WHEN each is put through the allowlist, developer mode on
    // THEN none passes
    for (const name of gone) expect(allowedGuideRoute(`/${name}`, true)).toBeNull();
  });

  it("reads the developer mode in force when none is given", () => {
    // GIVEN a button of the guide towards the Memory screen
    const raw: RawActionButton = {
      label: "Open Memory",
      action: "navigate",
      payload: { route: "/memory" },
    };

    // WHEN it is validated with the mode off, then on
    const off = validateActionButton(raw);
    developerMode.set(true);
    const on = validateActionButton(raw);

    // THEN it is only rendered with the mode on
    expect(off).toBeNull();
    expect(on?.target).toBe("memory");
  });
});

describe("apolliaGuide/actionButtons", () => {
  it("accepts allowlisted navigate routes", () => {
    const raw: RawActionButton = {
      label: "Ouvrir l'assistant",
      action: "navigate",
      payload: { route: "/automations?wizard=open" },
    };
    const safe = validateActionButton(raw);
    expect(safe).not.toBeNull();
    expect(safe!.target).toBe("automations");
    expect(safe!.query).toBe("wizard=open");
  });

  it("rejects unknown routes", () => {
    const raw: RawActionButton = {
      label: "x",
      action: "navigate",
      payload: { route: "/admin/destroy" },
    };
    expect(validateActionButton(raw)).toBeNull();
  });

  it("rejects unknown invoke commands", () => {
    const raw: RawActionButton = {
      label: "delete",
      action: "invoke",
      payload: { command: "delete_everything" },
    };
    expect(validateActionButton(raw)).toBeNull();
  });

  it("rejects buttons with empty labels", () => {
    const raw: RawActionButton = {
      label: "   ",
      action: "navigate",
      payload: { route: "/inbox" },
    };
    expect(validateActionButton(raw)).toBeNull();
  });

  it("sanitizeActionButtons caps at 3", () => {
    const raw: RawActionButton[] = Array.from({ length: 5 }, (_, i) => ({
      label: `btn ${i}`,
      action: "navigate" as const,
      payload: { route: "/inbox" },
    }));
    expect(sanitizeActionButtons(raw)).toHaveLength(3);
  });

  it("sanitizeActionButtons drops invalid items silently", () => {
    const raw: RawActionButton[] = [
      { label: "ok", action: "navigate", payload: { route: "/inbox" } },
      { label: "bad", action: "navigate", payload: { route: "/nope" } },
      { label: "ok2", action: "navigate", payload: { route: "/agents" } },
    ];
    const out = sanitizeActionButtons(raw);
    expect(out).toHaveLength(2);
    expect(out.map((b) => b.target)).toEqual(["inbox", "agents"]);
  });
});
