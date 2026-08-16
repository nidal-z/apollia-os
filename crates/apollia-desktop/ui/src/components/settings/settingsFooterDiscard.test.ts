import { describe, test, expect } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import en from "$lib/i18n/en.json";
import fr from "$lib/i18n/fr.json";

/**
 * Contract for the settings save footer's discarding button.
 *
 * The footer rendered by `SettingsSubPage.svelte` is shared by the three
 * settings pages that persist on demand (Profile, Dictation, Observability).
 * Its left button throws away everything typed since the last save, with no
 * confirmation and no way back. It used to carry `settings.reset`, whose value
 * was the exact string `common.cancel` carries on the dialogs that close
 * without changing anything, in the most discreet variant the button primitive
 * has. The product already names this action elsewhere, on the same data:
 * `settings.unsaved_dialog_discard`, in the destructive variant.
 *
 * The test environment is node (no DOM, components are not rendered, see
 * `vitest.config.ts`), so the button's presentation is pinned on the source
 * attributes and the wording on the catalogues.
 */

const LOCALES = [
  ["fr", fr],
  ["en", en],
] as const;

/**
 * The attribute block of the `<Button>` carrying `settings-subpage-reset`.
 *
 * The block is delimited by the line that closes the opening tag rather than by
 * the first `>` character: an attribute value holds an arrow function, so the
 * character alone would cut the block short.
 */
function resetButtonAttributes(): string {
  const source = readFileSync(
    join(__dirname, "SettingsSubPage.svelte"),
    "utf-8",
  );
  const segment = source
    .split("<Button")
    .find((part) => part.includes('data-testid="settings-subpage-reset"'));
  expect(
    segment,
    "SettingsSubPage.svelte should hold a Button tagged settings-subpage-reset",
  ).toBeDefined();
  // SAFETY of the non-null assertion: asserted defined on the line above.
  const opening = segment as string;
  const close = opening.search(/^\s*>\s*$/m);
  expect(close, "the Button opening tag should close on its own line").
    toBeGreaterThan(0);
  return opening.slice(0, close);
}

describe("settings footer discard button", () => {
  for (const [locale, catalogue] of LOCALES) {
    test(`${locale}: its label is not the word used to close without changing anything`, () => {
      // GIVEN the label the footer button renders, and the one 45 dialogs use
      // to close without acting
      const label = catalogue.settings.reset;
      const cancel = catalogue.common.cancel;

      // WHEN comparing them
      // THEN the destructive button does not borrow the harmless word
      expect(label).not.toBe(cancel);
    });

    test(`${locale}: its label is the one the product already gives this action`, () => {
      // GIVEN the footer label and the nav-guard dialog label, both discarding
      // the same unsaved edits
      const label = catalogue.settings.reset;
      const discard = catalogue.settings.unsaved_dialog_discard;

      // WHEN comparing them
      // THEN the same action reads the same on both surfaces
      expect(label).toBe(discard);
    });
  }

  test("it is rendered in the destructive variant", () => {
    // GIVEN the attributes of the footer button that throws the edits away
    const attributes = resetButtonAttributes();

    // WHEN reading the variant it asks the button primitive for
    // THEN it announces the destruction instead of hiding it in the most
    // discreet variant
    expect(attributes).toContain('variant="destructive"');
    expect(attributes).not.toContain('variant="ghost"');
  });
});
