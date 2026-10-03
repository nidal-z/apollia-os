import { describe, test, expect } from "vitest";
import { settingsToggleState } from "./SettingsToggle.svelte";
import en from "$lib/i18n/en.json";
import fr from "$lib/i18n/fr.json";

describe("SettingsToggle - settingsToggleState", () => {
  test("a value the policy imposes disables the switch and shows the policy label", () => {
    // GIVEN a setting whose source is the organization policy, on a row the
    // caller left enabled
    // WHEN the row state is computed
    const row = settingsToggleState("policy", false);
    // THEN the switch is disabled and the policy is named
    expect(row.disabled).toBe(true);
    expect(row.policyLabelKey).toBe("settings.policy_locked");
  });

  test("a value from the user or the default stays free", () => {
    // GIVEN the two other sources, and a row with no source at all
    // WHEN the row states are computed
    const rows = [
      settingsToggleState("user", false),
      settingsToggleState("default", false),
      settingsToggleState(undefined, false),
    ];
    // THEN none is disabled and none names the policy
    for (const row of rows) {
      expect(row).toEqual({ disabled: false, policyLabelKey: null });
    }
  });

  test("a caller that disables the row keeps it disabled without naming the policy", () => {
    // GIVEN a row disabled by its caller, from the user
    // WHEN the row state is computed
    const row = settingsToggleState("user", true);
    // THEN it stays disabled, with no policy mention
    expect(row).toEqual({ disabled: true, policyLabelKey: null });
  });

  test("the policy label exists in both catalogues, in each language", () => {
    // GIVEN the two catalogues
    // WHEN the policy label and its hint are looked up
    // THEN English reads "Enforced by policy", and French carries its own text
    expect(en.settings.policy_locked).toBe("Enforced by policy");
    expect(fr.settings.policy_locked.length).toBeGreaterThan(0);
    expect(fr.settings.policy_locked).not.toBe(en.settings.policy_locked);
    expect(en.settings.policy_locked_hint.length).toBeGreaterThan(0);
    expect(fr.settings.policy_locked_hint.length).toBeGreaterThan(0);
  });
});
