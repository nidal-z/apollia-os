import { describe, test, expect, vi, beforeAll } from "vitest";
import { render as renderToString } from "svelte/server";
import { locale, waitLocale } from "svelte-i18n";
import "$lib/i18n";

// Stores read `localStorage` at import time; the node environment has none.
vi.hoisted(() => {
  const memory = new Map<string, string>();
  globalThis.localStorage = {
    getItem: (k: string) => memory.get(k) ?? null,
    setItem: (k: string, v: string) => void memory.set(k, v),
    removeItem: (k: string) => void memory.delete(k),
    clear: () => memory.clear(),
    key: (i: number) => Array.from(memory.keys())[i] ?? null,
    get length() {
      return memory.size;
    },
  };
});

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => null) }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
  emit: vi.fn(async () => {}),
}));

// The tooltip primitive needs the provider the application shell mounts; here
// it only has to hand its trigger through, so the "?" is rendered bare.
vi.mock("$lib/components/ui/tooltip", () => ({
  Tooltip: (out: unknown, props: { children?: (out: unknown) => void }) => props.children?.(out),
}));

import ModelDefaults from "./ModelDefaults.svelte";
import { initialModelDefaults, type ModelDefaultsState } from "$lib/settings/modelDefaults";
import fr from "$lib/i18n/fr.json";
import en from "$lib/i18n/en.json";

/** The opening tag carrying `data-testid="<testid>"`, or `null`. */
function tagOf(html: string, testid: string): string | null {
  const match = html.match(new RegExp(`<[^>]*data-testid="${testid}"[^>]*>`));
  return match ? match[0] : null;
}

/** The opening tag of the radio of the choice `testid`, found by its id. */
function radioOf(html: string, testid: string): string {
  return html.match(new RegExp(`<button[^>]*id="${testid}-radio"[^>]*>`))?.[0] ?? "";
}

/** Whether an opening tag carries the `disabled` attribute. */
function isDisabled(tag: string | null): boolean {
  return tag !== null && /\sdisabled(=""|\s|\/?>)/.test(tag);
}

function page(state: ModelDefaultsState = initialModelDefaults()): string {
  return renderToString(ModelDefaults, { props: { initial: state } }).body;
}

/** The default state with `fields` fixed by the organization policy. */
function underPolicy(...fields: (keyof ModelDefaultsState["sources"])[]): ModelDefaultsState {
  const state = initialModelDefaults();
  for (const field of fields) state.sources[field] = "policy";
  return state;
}

beforeAll(async () => {
  locale.set("fr");
  await waitLocale();
});

describe("Settings > Model defaults", () => {
  test("shows its blocks in the order of the reference screen", () => {
    // GIVEN the page of a machine nobody configured
    const html = page();

    // WHEN the position of each block is read
    const order = [
      "model-defaults-images",
      "model-defaults-context",
      "model-defaults-guardrails",
      "model-defaults-bypass",
      "model-defaults-unload",
    ].map((testid) => html.indexOf(`data-testid="${testid}"`));

    // THEN every block is there, images first, then the context, the
    //      guardrails, the bypass, and the unload delay
    expect(order.every((at) => at >= 0)).toBe(true);
    expect([...order].sort((a, b) => a - b)).toEqual(order);
  });

  test("offers the context as two radios, the custom value with its field", () => {
    // GIVEN the page of a machine nobody configured
    const html = page();

    // WHEN the context block is read
    const custom = tagOf(html, "model-defaults-context-custom-row");
    const maximum = tagOf(html, "model-defaults-context-model-max-row");

    // THEN the custom value is chosen and carries a field, the model maximum
    //      is not and carries none
    expect(custom).toContain('data-checked="true"');
    expect(maximum).toContain('data-checked="false"');
    expect(tagOf(html, "model-defaults-context-custom-field")).not.toBeNull();
    expect(tagOf(html, "model-defaults-context-model-max-field")).toBeNull();
    expect(html).toContain(fr.settings.model_defaults.context.custom);
    expect(html).toContain(fr.settings.model_defaults.context.model_max);
  });

  test("offers five explained guardrails with a help mark, balanced chosen", () => {
    // GIVEN the page of a machine nobody configured
    const html = page();

    // WHEN the guardrail block is read
    const rows = ["disabled", "lenient", "balanced", "strict", "ceiling"].map((level) =>
      tagOf(html, `model-defaults-guardrail-${level}-row`),
    );

    // THEN the five levels are there, each explained, balanced is chosen, the
    //      ceiling carries its field in gigabytes, and the "?" is offered
    expect(rows.every((row) => row !== null)).toBe(true);
    expect(rows.map((row) => row?.includes('data-checked="true"'))).toEqual([
      false,
      false,
      true,
      false,
      false,
    ]);
    for (const level of ["disabled", "lenient", "balanced", "strict", "ceiling"] as const) {
      expect(html).toContain(fr.settings.model_defaults.guardrails[`${level}_hint`]);
    }
    expect(tagOf(html, "model-defaults-guardrail-ceiling-field")).not.toBeNull();
    expect(tagOf(html, "model-defaults-guardrails-help")).not.toBeNull();
  });

  test("offers the bypass as load anyway with two explained radios", () => {
    // GIVEN the page of a machine nobody configured
    const html = page();

    // WHEN the bypass block is read
    // THEN "load anyway" heads it, the explicit confirmation is chosen, and
    //      no restriction is offered as not recommended
    expect(html).toContain(fr.settings.model_defaults.bypass.load_anyway);
    expect(tagOf(html, "model-defaults-bypass-confirm-row")).toContain('data-checked="true"');
    expect(tagOf(html, "model-defaults-bypass-unrestricted-row")).toContain('data-checked="false"');
    expect(html).toContain(fr.settings.model_defaults.bypass.unrestricted);
  });

  test("disables the guardrail selector under a policy and names it", () => {
    // GIVEN a test policy that fixes the guardrail level
    const html = page(underPolicy("guardrailLevel"));

    // WHEN the guardrail block is read
    const selector = tagOf(html, "model-defaults-guardrail-selector");
    const mention = html.indexOf('data-testid="model-defaults-guardrails-policy"');

    // THEN the selector is disabled, every radio with it, and the block
    //      names the policy; the context stays free
    expect(selector).toContain('aria-disabled="true"');
    expect(mention).toBeGreaterThan(-1);
    expect(html.slice(mention, mention + 2000)).toContain(fr.settings.policy_locked);
    expect(isDisabled(radioOf(html, "model-defaults-guardrail-strict"))).toBe(true);
    expect(tagOf(html, "model-defaults-context-policy")).toBeNull();
    expect(radioOf(html, "model-defaults-context-custom")).not.toBe("");
    expect(isDisabled(radioOf(html, "model-defaults-context-custom"))).toBe(false);
  });

  test("shows the unload delay at sixty minutes, and locked under a policy", () => {
    // GIVEN the page of a machine nobody configured, then under a policy that
    //       fixes the delay
    const free = page();
    const locked = page(underPolicy("idleUnload"));

    // WHEN the unload block is read in each
    const field = tagOf(free, "model-defaults-unload-after-field");
    const lockedField = tagOf(locked, "model-defaults-unload-after-field");

    // THEN the field shows sixty and is editable; under the policy it is
    //      disabled and the block names the policy
    expect(free).toContain(fr.settings.model_defaults.unload.label);
    expect(field).toContain('value="60"');
    expect(isDisabled(field)).toBe(false);
    expect(isDisabled(lockedField)).toBe(true);
    expect(tagOf(free, "model-defaults-unload-policy")).toBeNull();
    expect(tagOf(locked, "model-defaults-unload-policy")).not.toBeNull();
  });

  test("starts with the image ceiling, on at 2048 pixels, lockable by a policy", () => {
    // GIVEN the page of a machine nobody configured, then under a policy that
    //       fixes the image ceiling
    const free = page();
    const locked = page(underPolicy("imageLimit"));

    // WHEN the image block is read in each
    // THEN it holds images to 2048 pixels with its switch on; under
    //      the policy the field and the switch are disabled
    expect(free).toContain(fr.settings.model_defaults.images.never_exceed);
    expect(tagOf(free, "model-defaults-image-side")).toContain('value="2048"');
    expect(tagOf(free, "model-defaults-image-limit")).toContain('aria-checked="true"');
    expect(isDisabled(tagOf(free, "model-defaults-image-side"))).toBe(false);
    expect(isDisabled(tagOf(locked, "model-defaults-image-side"))).toBe(true);
    expect(isDisabled(tagOf(locked, "model-defaults-image-limit"))).toBe(true);
    expect(tagOf(locked, "model-defaults-images-policy")).not.toBeNull();
  });

  test("locks the bypass under a policy that fixes it", () => {
    // GIVEN a test policy that fixes the bypass
    const html = page(underPolicy("bypass"));

    // WHEN the bypass block is read
    // THEN it names the policy and its radios are disabled
    expect(tagOf(html, "model-defaults-bypass-policy")).not.toBeNull();
    expect(isDisabled(radioOf(html, "model-defaults-bypass-unrestricted"))).toBe(true);
  });

  test("carries the same keys in French and in English", () => {
    // GIVEN the two catalogues
    // WHEN the keys of the screen are flattened in each
    const keys = (node: unknown, prefix = ""): string[] =>
      typeof node === "object" && node !== null
        ? Object.entries(node).flatMap(([k, v]) => keys(v, `${prefix}${k}.`))
        : [prefix];

    // THEN they are the same
    expect(keys(en.settings.model_defaults)).toEqual(keys(fr.settings.model_defaults));
  });
});
