import { describe, test, expect, beforeAll } from "vitest";
import { get } from "svelte/store";
import { locale, waitLocale } from "svelte-i18n";
import "./index";
import en from "./en.json";
import fr from "./fr.json";

/**
 * Validates that switching the active locale via `svelte-i18n`'s `locale`
 * store immediately updates resolved values — the guarantee US-SP42-008
 * asks for: changing locale in dev swaps strings without a reload.
 *
 * Uses a handful of spec-called-out keys (Workspace / Thinking / Libre
 * / Plan cache / icon-only aria-label) so regressions on those specific
 * findings fail loudly.
 */

const SPEC_KEYS = [
  { key: "common.workspace", en: "Workspace", fr: "Espace de travail" },
  { key: "chat.thinking", en: "Thinking...", fr: "Réflexion en cours..." },
  { key: "chat.legend_free", en: "Free", fr: "Libre" },
  { key: "a11y.close", en: "Close", fr: "Fermer" },
  {
    key: "observability.plan_cache.title",
    en: "Plan cache",
    fr: "Cache de plans",
  },
];

function lookup(locale: "en" | "fr", key: string): string {
  const source = locale === "en" ? en : fr;
  return key
    .split(".")
    .reduce<unknown>((node, segment) => {
      if (typeof node !== "object" || node === null) return undefined;
      return (node as Record<string, unknown>)[segment];
    }, source) as string;
}

beforeAll(async () => {
  locale.set("en");
  await waitLocale();
});

describe("locale switching", () => {
  test("EN catalog returns the expected spec-called-out values", async () => {
    locale.set("en");
    await waitLocale();
    expect(get(locale)).toBe("en");
    for (const { key, en: expected } of SPEC_KEYS) {
      expect(lookup("en", key), key).toBe(expected);
    }
  });

  test("FR catalog returns the expected spec-called-out values", async () => {
    locale.set("fr");
    await waitLocale();
    expect(get(locale)).toBe("fr");
    for (const { key, fr: expected } of SPEC_KEYS) {
      expect(lookup("fr", key), key).toBe(expected);
    }
  });

  test("toggling EN → FR → EN returns distinct values", async () => {
    locale.set("en");
    await waitLocale();
    const before = lookup("en", "common.workspace");
    locale.set("fr");
    await waitLocale();
    const frValue = lookup("fr", "common.workspace");
    locale.set("en");
    await waitLocale();
    const after = lookup("en", "common.workspace");
    expect(before).toBe("Workspace");
    expect(frValue).toBe("Espace de travail");
    expect(after).toBe("Workspace");
    expect(before).not.toBe(frValue);
  });
});
