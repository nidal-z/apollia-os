import { describe, it, expect, beforeEach } from "vitest";
import { get } from "svelte/store";
import {
  parseSettingsPath,
  setSettingsSubRoute,
  settingsSubRoute,
  SETTINGS_SUB_ROUTES,
  DEFAULT_SETTINGS_SUB_ROUTE,
} from "./router";

describe("settings router", () => {
  beforeEach(() => {
    settingsSubRoute.set(DEFAULT_SETTINGS_SUB_ROUTE);
  });

  it("exposes the canonical sub-routes in order", () => {
    // GIVEN the settings store
    // WHEN its canonical sub-routes are read
    // THEN they follow the navigation column: settings, system, then the leaving sections
    expect(SETTINGS_SUB_ROUTES).toEqual([
      "general",
      "appearance",
      "profile",
      "chat",
      "stt",
      "model-defaults",
      "developer",
      "configuration",
      "engines",
      "hardware",
      "llm",
      "model-hub",
      "tools",
      "permissions",
    ]);
  });

  it("defaults to 'general'", () => {
    // GIVEN a fresh store WHEN its default is read THEN it is General
    expect(DEFAULT_SETTINGS_SUB_ROUTE).toBe("general");
    expect(get(settingsSubRoute)).toBe("general");
  });

  it("parseSettingsPath resolves known sub-routes", () => {
    expect(parseSettingsPath("/settings/stt")).toBe("stt");
    expect(parseSettingsPath("settings/llm")).toBe("llm");
    expect(parseSettingsPath("/settings/engines")).toBe("engines");
  });

  it.each([
    ["about", "general"],
    ["help", "general"],
    ["danger", "general"],
    ["shortcuts", "chat"],
    ["system", "hardware"],
  ])("a former section (%s) opens the section that now holds it (%s)", (former, home) => {
    // GIVEN a caller that still names a section the grid no longer lists
    settingsSubRoute.set("profile");

    // WHEN it navigates there
    const resolved = setSettingsSubRoute(former);

    // THEN the store lands on the section that carries the moved content
    expect(resolved).toBe(home);
    expect(get(settingsSubRoute)).toBe(home);
  });

  it("parseSettingsPath returns null for bare /settings (triggers redirect)", () => {
    expect(parseSettingsPath("/settings")).toBeNull();
    expect(parseSettingsPath("/settings/")).toBeNull();
  });

  it("parseSettingsPath returns null for unknown sub-routes", () => {
    expect(parseSettingsPath("/settings/unknown")).toBeNull();
    expect(parseSettingsPath("/other")).toBeNull();
  });

  it("setSettingsSubRoute updates the store and rejects unknown routes (fallback to default)", () => {
    setSettingsSubRoute("stt");
    expect(get(settingsSubRoute)).toBe("stt");
    setSettingsSubRoute("unknown");
    expect(get(settingsSubRoute)).toBe(DEFAULT_SETTINGS_SUB_ROUTE);
  });

  it("switching sub-routes does not create a new store instance (shared across mounts)", () => {
    setSettingsSubRoute("stt");
    const stateA = get(settingsSubRoute);
    setSettingsSubRoute("llm");
    const stateB = get(settingsSubRoute);
    expect(stateA).toBe("stt");
    expect(stateB).toBe("llm");
  });
});
