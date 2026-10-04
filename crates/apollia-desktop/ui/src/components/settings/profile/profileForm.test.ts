import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, test, vi } from "vitest";
import type {
  ProfileEntryView,
  ProfileFieldView,
  ProfileSummaryView,
} from "$lib/ipc/profile";
import {
  addFact,
  applyReset,
  changedFields,
  copyFactValue,
  editFact,
  factKeyFor,
  fieldHelp,
  fieldLabel,
  profileAfterDelete,
  profileMenu,
  resetChoices,
  shouldOfferEnrichment,
} from "./profileForm";

function profile(id: string, name: string, isDefault: boolean): ProfileSummaryView {
  return {
    id,
    name,
    is_default: isDefault,
    detail: null,
    created_at: "2026-10-04T00:00:00Z",
    updated_at: "2026-10-04T00:00:00Z",
  };
}

function field(key: string, value = ""): ProfileFieldView {
  return {
    key,
    label_fr: `fr:${key}`,
    label_en: `en:${key}`,
    help_fr: `aide:${key}`,
    help_en: `help:${key}`,
    long_text: key === "free_text",
    value,
  };
}

function fact(key: string, value: string): ProfileEntryView {
  return {
    key,
    value,
    written_by: "user",
    created_at: "2026-10-04T00:00:00Z",
    updated_at: "2026-10-04T00:00:00Z",
    in_schema: false,
  };
}

const SIX = ["name", "role", "function", "tools", "preferences", "free_text"];
const RESPONSABLE = profile("p-si", "Responsable SI", true);
const FORMATEUR = profile("p-fo", "Formateur", false);

describe("the profiles (criterion 1)", () => {
  test("a profile that is not the default offers rename, set as default, reset and delete", () => {
    // GIVEN the profile "Formateur", not the default
    // WHEN its menu is built
    const menu = profileMenu(FORMATEUR);

    // THEN every action is there and enabled
    expect(menu.map((e) => e.action)).toEqual(["rename", "set_default", "reset", "delete"]);
    expect(menu.every((e) => !e.disabled)).toBe(true);
  });

  test("deleting the default profile is refused with a readable reason", () => {
    // GIVEN the default profile
    // WHEN its menu is built
    const menu = profileMenu(RESPONSABLE);
    const deletion = menu.find((e) => e.action === "delete");

    // THEN it cannot be set as default again, and its deletion is disabled with
    // the reason a user reads
    expect(menu.map((e) => e.action)).not.toContain("set_default");
    expect(deletion?.disabled).toBe(true);
    expect(deletion?.reasonKey).toBe("settings.profile.menu.delete_refused");
    const catalogue = (lang: string) =>
      JSON.parse(readFileSync(resolve(__dirname, `../../../lib/i18n/${lang}.json`), "utf8"));
    const en = catalogue("en").settings.profile.menu.delete_refused;
    const fr = catalogue("fr").settings.profile.menu.delete_refused;
    expect(en).toContain("cannot be deleted");
    expect(fr).toBeTruthy();
    expect(fr).not.toBe(en);
  });

  test("once a profile is deleted the default one is displayed", () => {
    // GIVEN the two profiles, "Formateur" displayed
    // WHEN "Formateur" is deleted
    const next = profileAfterDelete([RESPONSABLE, FORMATEUR], FORMATEUR.id);

    // THEN the default profile is the one displayed
    expect(next).toBe(RESPONSABLE.id);
  });
});

describe("the form (criterion 2)", () => {
  test("labels and help come from the fields Rust sends, in the interface locale", () => {
    // GIVEN a field as Rust describes it
    const role = field("role");

    // WHEN it is labelled in French and in English
    // THEN each locale reads its own words
    expect(fieldLabel(role, "fr-FR")).toBe("fr:role");
    expect(fieldLabel(role, "en")).toBe("en:role");
    expect(fieldHelp(role, "fr")).toBe("aide:role");
  });

  test("only changed fields are saved, trimmed, and an emptied field is sent empty", () => {
    // GIVEN the six fields, name and role filled
    const fields = SIX.map((k) => field(k, k === "name" ? "Claire" : k === "role" ? "RSI" : ""));

    // WHEN the role is emptied and the free text written
    const changed = changedFields(fields, {
      name: "Claire",
      role: "  ",
      free_text: " Je facture le 5 ",
    });

    // THEN exactly those two are saved
    expect(changed).toEqual({ role: "", free_text: "Je facture le 5" });
  });

  test("the enrichment banner waits for name and role, then for an empty field", () => {
    // GIVEN a profile with name and role, and one with every field
    const partial = SIX.map((k) => field(k, k === "name" || k === "role" ? "x" : ""));
    const full = SIX.map((k) => field(k, "x"));

    // WHEN the banner decision is read
    // THEN it shows on the partial profile only, and never on an empty one
    expect(shouldOfferEnrichment(partial)).toBe(true);
    expect(shouldOfferEnrichment(full)).toBe(false);
    expect(shouldOfferEnrichment(SIX.map((k) => field(k)))).toBe(false);
  });

  test("the form carries no key of the former schema", () => {
    // GIVEN the sources of the profile screen
    const files = [
      "profileForm.ts",
      "ProfileFieldsCard.svelte",
      "ProfileFieldGrid.svelte",
      "ProfileListCard.svelte",
      "ProfileEnrichmentBanner.svelte",
      "../../../routes/settings/Profile.svelte",
    ].map((f) => readFileSync(resolve(__dirname, f), "utf8"));

    // WHEN they are searched for the keys the form used to copy by hand
    // THEN none remains
    for (const source of files) {
      expect(source).not.toMatch(/tech\.stack|agents\.hitl|constraints\.sovereignty|tools\.daily/);
    }
  });
});

describe("the facts (criterion 6)", () => {
  test("adding a fact without developer mode derives its key from the sentence", async () => {
    // GIVEN the displayed profile, one fact already under the derived key
    const set = vi.fn(async (_id: string, key: string, value: string) => fact(key, value));
    const existing = [fact("fact.je-facture-mes-clients-le-5", "Je facture mes clients le 5")];

    // WHEN the same sentence is added again
    const added = await addFact("p-fo", " Je facture mes clients le 5 ", null, existing, { set });

    // THEN it is written to that profile under a new key, value trimmed
    expect(set).toHaveBeenCalledWith(
      "p-fo",
      "fact.je-facture-mes-clients-le-5-2",
      "Je facture mes clients le 5",
    );
    expect(added?.value).toBe("Je facture mes clients le 5");
  });

  test("adding a fact in developer mode uses the typed key, and an empty field writes nothing", async () => {
    // GIVEN the add form in developer mode
    const set = vi.fn(async (_id: string, key: string, value: string) => fact(key, value));

    // WHEN a key and a value are typed, then an empty key, then an empty value
    await addFact("p-fo", "Paris", "prefs.timezone", [], { set });
    const noKey = await addFact("p-fo", "Paris", "  ", [], { set });
    const noValue = await addFact("p-fo", " ", null, [], { set });

    // THEN only the first reaches the store
    expect(set).toHaveBeenCalledTimes(1);
    expect(set).toHaveBeenCalledWith("p-fo", "prefs.timezone", "Paris");
    expect(noKey).toBeNull();
    expect(noValue).toBeNull();
  });

  test("a fact key stays readable without accents, and never empty", () => {
    // GIVEN sentences with accents and with no letter at all
    // WHEN keys are derived
    // THEN they read as words, and a sentence of symbols gets a fallback key
    expect(factKeyFor("Zo\u00eb prefers \u00c5ngstr\u00f6m units", [])).toBe("fact.zoe-prefers-angstrom-units");
    expect(factKeyFor("!!!", [])).toBe("fact.note");
  });

  test("saving an edit writes the new value; an unchanged or empty one writes nothing", async () => {
    // GIVEN a fact being edited
    const set = vi.fn(async (_id: string, key: string, value: string) => fact(key, value));
    const timezone = fact("prefs.timezone", "Paris");

    // WHEN it is saved with a new value, then unchanged, then emptied (cancel)
    const saved = await editFact("p-fo", timezone, " Lyon ", { set });
    const unchanged = await editFact("p-fo", timezone, "Paris", { set });
    const emptied = await editFact("p-fo", timezone, "", { set });

    // THEN only the new value reaches the profile
    expect(set).toHaveBeenCalledTimes(1);
    expect(set).toHaveBeenCalledWith("p-fo", "prefs.timezone", "Lyon");
    expect(saved?.value).toBe("Lyon");
    expect(unchanged).toBeNull();
    expect(emptied).toBeNull();
  });

  test("copying a fact copies its value, never its key", async () => {
    // GIVEN a fact and a clipboard
    const writeText = vi.fn(async () => undefined);

    // WHEN its value is copied from the context menu
    await copyFactValue(fact("prefs.timezone", "Paris"), { writeText });

    // THEN the clipboard holds the value
    expect(writeText).toHaveBeenCalledWith("Paris");
  });

  test("the row's menu offers edit, copy and delete, and deletes through the profile", () => {
    // GIVEN the fact row
    const row = readFileSync(resolve(__dirname, "ProfileFactRow.svelte"), "utf8");

    // WHEN its context menu and its delete path are read
    // THEN the three entries are there and the deletion names the profile
    expect(row).toMatch(/id: "edit"[\s\S]*id: "copy"[\s\S]*id: "delete"/);
    expect(row).toContain("deleteProfileFact(profileId, key)");
  });
});

describe("the reset (criterion 7)", () => {
  test("a profile that is not the default is reset alone, without onboarding", async () => {
    // GIVEN "Formateur" and the effects of a reset
    const reset = vi.fn(async () => 3);
    const onboard = vi.fn(async () => undefined);

    // WHEN its reset is confirmed, even asking for the onboarding
    const choices = resetChoices(FORMATEUR);
    await applyReset(FORMATEUR, "reset_and_onboard", { reset, onboard });

    // THEN only the reset was offered and only the reset ran
    expect(choices).toEqual(["reset"]);
    expect(reset).toHaveBeenCalledWith("p-fo");
    expect(onboard).not.toHaveBeenCalled();
  });

  test("the default profile offers to get acquainted again, and runs it after the reset", async () => {
    // GIVEN the default profile and the effects of a reset
    const order: string[] = [];
    const reset = vi.fn(async () => order.push("reset"));
    const onboard = vi.fn(async () => order.push("onboard"));

    // WHEN its reset is confirmed with the onboarding, then alone
    const choices = resetChoices(RESPONSABLE);
    await applyReset(RESPONSABLE, "reset_and_onboard", { reset, onboard });
    await applyReset(RESPONSABLE, "reset", { reset, onboard });

    // THEN both choices were offered, the onboarding followed the first reset
    // and not the second
    expect(choices).toEqual(["reset", "reset_and_onboard"]);
    expect(order).toEqual(["reset", "onboard", "reset"]);
  });

  test("a failed reset never launches the onboarding", async () => {
    // GIVEN a reset the store refuses
    const reset = vi.fn(async () => {
      throw new Error("NOT_INITIALIZED");
    });
    const onboard = vi.fn(async () => undefined);

    // WHEN the default profile is reset with the onboarding
    const outcome = applyReset(RESPONSABLE, "reset_and_onboard", { reset, onboard });

    // THEN the error surfaces and the onboarding did not start
    await expect(outcome).rejects.toThrow("NOT_INITIALIZED");
    expect(onboard).not.toHaveBeenCalled();
  });
});
