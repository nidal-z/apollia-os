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
    expect(SETTINGS_SUB_ROUTES).toEqual([
      "appearance",
      "profile",
      "stt",
      "llm",
      "model-hub",
      "configuration",
      "tools",
      "permissions",
      "system",
      "memories",
      "shortcuts",
      "danger",
    ]);
  });

  it("defaults to 'appearance'", () => {
    expect(DEFAULT_SETTINGS_SUB_ROUTE).toBe("appearance");
    expect(get(settingsSubRoute)).toBe("appearance");
  });

  it("parseSettingsPath resolves known sub-routes", () => {
    expect(parseSettingsPath("/settings/stt")).toBe("stt");
    expect(parseSettingsPath("settings/llm")).toBe("llm");
    expect(parseSettingsPath("/settings/danger")).toBe("danger");
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
