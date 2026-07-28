import { describe, expect, it } from "vitest";
import { TOURS, tourById } from "./catalog";
import { shouldRetainStep } from "./engine";
import type { TourStep } from "./types";
import en from "$lib/i18n/en.json";
import fr from "$lib/i18n/fr.json";

/** Resolves a dotted i18n path, returning `undefined` when any segment is missing. */
function lookup(bundle: unknown, path: string): unknown {
  return path.split(".").reduce<unknown>((node, segment) => {
    if (typeof node !== "object" || node === null) return undefined;
    return (node as Record<string, unknown>)[segment];
  }, bundle);
}

const step = (overrides: Partial<TourStep> = {}): TourStep => ({
  id: "s",
  anchor: { kind: "testid", value: "topbar-search" },
  titleKey: "tour.landmarks.palette.title",
  bodyKey: "tour.landmarks.palette.body",
  ...overrides,
});

describe("tour pre-flight - shouldRetainStep", () => {
  it("drops a required step whose anchor is absent", () => {
    // GIVEN a required step and an anchor that does not resolve
    // WHEN pre-flight decides
    const retained = shouldRetainStep(step(), false);
    // THEN the step is dropped, so the counter never promises a step the user
    // will not see
    expect(retained).toBe(false);
  });

  it("keeps a required step whose anchor is present", () => {
    // GIVEN a required step and a resolving anchor
    // WHEN pre-flight decides
    const retained = shouldRetainStep(step(), true);
    // THEN the step is kept
    expect(retained).toBe(true);
  });

  it("keeps an optional step even when its anchor is absent", () => {
    // GIVEN an optional step whose anchor appears only after an action
    // WHEN pre-flight decides
    const retained = shouldRetainStep(step({ optional: true }), false);
    // THEN it survives and will fall back to the anchorless presentation
    expect(retained).toBe(true);
  });

  it("keeps a step that waits for its anchor", () => {
    // GIVEN the approval annotation, whose card does not exist yet by design
    // WHEN pre-flight decides
    const retained = shouldRetainStep(step({ awaitAnchor: true }), false);
    // THEN it survives: dropping it would defeat its whole purpose
    expect(retained).toBe(true);
  });

  it("keeps a step that has no anchor at all", () => {
    // GIVEN a step that explains rather than points
    // WHEN pre-flight decides
    const retained = shouldRetainStep(step({ anchor: null }), false);
    // THEN it survives
    expect(retained).toBe(true);
  });
});

describe("tour catalogue", () => {
  it("exposes every tour by its own identifier", () => {
    // GIVEN the catalogue
    // WHEN each entry is looked up
    // THEN the key and the definition identifier agree, so a rename cannot
    // silently desynchronise the two
    for (const [id, definition] of Object.entries(TOURS)) {
      expect(definition.id).toBe(id);
      expect(tourById(definition.id)).toBe(definition);
    }
  });

  it("gives every tour at least one step", () => {
    // GIVEN the catalogue
    // WHEN step counts are read
    // THEN none is empty: a tour with no step could never start
    for (const definition of Object.values(TOURS)) {
      expect(definition.steps.length).toBeGreaterThan(0);
    }
  });

  it("keeps step identifiers unique inside a tour", () => {
    // GIVEN the catalogue
    // WHEN step identifiers are collected per tour
    // THEN they are unique, since persistence keys on them
    for (const definition of Object.values(TOURS)) {
      const ids = definition.steps.map((s) => s.id);
      expect(new Set(ids).size).toBe(ids.length);
    }
  });

  it("only ever waits for an anchor on a step that is also optional", () => {
    // GIVEN the catalogue
    // WHEN awaiting steps are inspected
    // THEN each is optional too: an anchor that may never appear must not be
    // able to strand the user on a step with no way to move on meaningfully
    for (const definition of Object.values(TOURS)) {
      for (const s of definition.steps) {
        if (s.awaitAnchor === true) expect(s.optional).toBe(true);
      }
    }
  });

  it("resolves every copy key in both locales", () => {
    // GIVEN every key the catalogue references
    const paths: string[] = [];
    for (const definition of Object.values(TOURS)) {
      paths.push(definition.titleKey);
      for (const s of definition.steps) paths.push(s.titleKey, s.bodyKey);
    }

    // WHEN each is looked up in the two bundles
    const missingEn = paths.filter((path) => typeof lookup(en, path) !== "string");
    const missingFr = paths.filter((path) => typeof lookup(fr, path) !== "string");

    // THEN none is missing. The previous runner shipped a double `.title`
    // suffix, so every card rendered its raw step id with an empty body; this
    // is the guard that would have caught it.
    expect(missingEn).toEqual([]);
    expect(missingFr).toEqual([]);
  });

  it("routes the annotated tour without a spotlight and the rest with one", () => {
    // GIVEN the catalogue
    // WHEN presentations are inspected
    // THEN only the first-result tour is annotated: it points at a live
    // approval card the user must remain able to answer
    const annotated = Object.values(TOURS).filter((d) => d.presentation === "annotated");
    expect(annotated.map((d) => d.id)).toEqual(["first-result"]);
  });
});
