import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, test, expect } from "vitest";
import { parse } from "svelte/compiler";
import { llmSectionView, sttSectionView } from "./OnboardingAiSetup.svelte";

// DOM rendering is exercised by the gestural corpus of scripts/automation
// (vitest runs in `node`). These tests lock the branch decisions the template
// consumes: which regions of the AI-setup step render, and above all which of
// them may never disappear. The last block closes the link the two previous
// ones leave open, by reading the template itself: a decision function is only
// worth its tests if the markup actually hangs off it.

describe("OnboardingAiSetup - llmSectionView", () => {
  test("an empty scan shows the hint and the three add means", () => {
    // GIVEN a machine where the scan found no GGUF file and nothing has been
    // configured during this session
    const view = llmSectionView(0, false);

    // THEN the step explains where to put a model and offers the three ways of
    // adding one
    expect(view.showEmptyHint).toBe(true);
    expect(view.showAddMeans).toBe(true);
    expect(view.showDetectedList).toBe(false);
    expect(view.showSuccessRow).toBe(false);
  });

  test("a non-empty scan keeps the three add means reachable", () => {
    // GIVEN a scan that found two GGUF files on disk
    const view = llmSectionView(2, false);

    // THEN the detected list is offered, and so are the three add means: one
    // file on disk must not close the door to the catalogue and the search
    expect(view.showDetectedList).toBe(true);
    expect(view.showAddMeans).toBe(true);
    // AND the "nothing found" hint is gone, it no longer describes the state
    expect(view.showEmptyHint).toBe(false);
  });

  test("a first successful import does not remove the button that allowed it", () => {
    // GIVEN an engine wired up during this session, the scan having refreshed
    const view = llmSectionView(1, true);

    // THEN the confirmation row appears
    expect(view.showSuccessRow).toBe(true);
    // AND the add means are still reachable: importing one engine is exactly
    // when an operator may want to import another
    expect(view.showAddMeans).toBe(true);
    expect(view.showDetectedList).toBe(true);
    expect(view.showEmptyHint).toBe(false);
  });

  test("a session success with an empty scan still hides the empty hint", () => {
    // GIVEN a configured engine that the scan does not list (cloud backend
    // wired up, or a scan that has not refreshed yet)
    const view = llmSectionView(0, true);

    // THEN the step does not claim that nothing was found
    expect(view.showEmptyHint).toBe(false);
    expect(view.showSuccessRow).toBe(true);
    expect(view.showAddMeans).toBe(true);
  });
});

describe("OnboardingAiSetup - sttSectionView", () => {
  test("an empty scan shows the hint, the catalogue and the add mean", () => {
    // GIVEN no Whisper model on disk, dictation therefore off
    const view = sttSectionView(0, false);

    // THEN the section explains the state and offers both ways in
    expect(view.showEmptyHint).toBe(true);
    expect(view.showCuratedList).toBe(true);
    expect(view.showAddMean).toBe(true);
    expect(view.showDetectedList).toBe(false);
    // AND the hotkey block stays out: there is nothing to dictate with
    expect(view.showHotkeyBlock).toBe(false);
  });

  test("models present with dictation off still render the list and the add mean", () => {
    // GIVEN Whisper models on disk and a dictation toggle left off, which is
    // the default whenever the first model found is not the recommended one
    const view = sttSectionView(2, false);

    // THEN the section still has a body: the list and the way to add a model
    expect(view.showDetectedList).toBe(true);
    expect(view.showAddMean).toBe(true);
    // AND only the hotkey and live-test block, which drives a running
    // dictation, follows the toggle
    expect(view.showHotkeyBlock).toBe(false);
    expect(view.showEmptyHint).toBe(false);
  });

  test("models present with dictation on add the hotkey block", () => {
    // GIVEN the same models and dictation enabled
    const view = sttSectionView(2, true);

    // THEN everything the previous case rendered is still there, plus the
    // hotkey, microphone and live-test block
    expect(view.showDetectedList).toBe(true);
    expect(view.showAddMean).toBe(true);
    expect(view.showHotkeyBlock).toBe(true);
  });
});

// ── The template hangs off the decision, not off the raw scan state ────────
//
// The two blocks above test functions. What closed the door on the operator
// was the markup: the add means lived inside `{#if ggufModels.length === 0 &&
// !llmSuccess}` and the voice list inside `{:else if sttEnabled}`. So the
// template is parsed and each region is asked which conditions stand above it.

const SOURCE = readFileSync(
  fileURLToPath(new URL("./OnboardingAiSetup.svelte", import.meta.url)),
  "utf8",
);
const TEMPLATE = parse(SOURCE, { modern: true }).fragment;

/** Reactive state that must never gate a list or a way of adding a model. */
const SCAN_STATE = /ggufModels|whisperModels|llmSuccess|sttEnabled/;

/**
 * Source text of every `{#if}` condition standing above the element carrying
 * `testid`, outermost first. `null` when no such element exists, so a renamed
 * or deleted region fails loudly instead of passing on an empty list.
 */
function conditionsAbove(testid: string): string[] | null {
  let hit: string[] | null = null;

  const walk = (node: unknown, stack: string[]): void => {
    if (Array.isArray(node)) {
      for (const child of node) walk(child, stack);
      return;
    }
    if (node === null || typeof node !== "object") return;
    const record = node as Record<string, unknown>;

    if (record.type === "Attribute" && record.name === "data-testid") {
      const value = record.value;
      const first = Array.isArray(value) ? (value[0] as Record<string, unknown>) : null;
      if (first?.type === "Text" && first.data === testid) hit = stack;
    }

    let inner = stack;
    if (record.type === "IfBlock") {
      const condition = record.test as { start: number; end: number };
      inner = [...stack, SOURCE.slice(condition.start, condition.end)];
    }

    for (const key of Object.keys(record)) {
      if (key === "parent") continue;
      walk(record[key], inner);
    }
  };

  walk(TEMPLATE, []);
  return hit;
}

describe("OnboardingAiSetup - template branches", () => {
  test("the walker reads real conditions, and the matcher matches the old ones", () => {
    // GIVEN two regions that legitimately keep a condition
    // THEN the walker returns their condition source, so an empty answer below
    // cannot be an artefact of a walker that finds nothing
    expect(conditionsAbove("llm-download-progress")).toContain("llmDownloadId");
    expect(conditionsAbove("stt-hotkey-block")).toContain("sttView.showHotkeyBlock");

    // AND the matcher does match the conditions this lot removed, verbatim
    expect(SCAN_STATE.test("ggufModels.length === 0 && !llmSuccess")).toBe(true);
    expect(SCAN_STATE.test("sttEnabled")).toBe(true);

    // AND an absent region is reported as absent, not as unconditioned
    expect(conditionsAbove("no-such-testid")).toBeNull();
  });

  test("no way of adding a model is gated by what the scan found", () => {
    // GIVEN the three language-engine add means and the voice one
    for (const testid of [
      "llm-load-model-btn",
      "curated-llm-list",
      "search-results",
      "stt-load-model-btn",
    ]) {
      const conditions = conditionsAbove(testid);
      // THEN the region exists
      expect(conditions, testid).not.toBeNull();
      // AND nothing above it depends on the scan or on the dictation toggle
      expect(conditions?.filter((c) => SCAN_STATE.test(c)), testid).toEqual([]);
    }
  });

  test("neither detected list is gated by the scan state or the toggle", () => {
    // GIVEN the two lists of models found on disk
    for (const testid of ["llm-model-list", "whisper-model-list"]) {
      const conditions = conditionsAbove(testid);
      // THEN they render off their own decision flag only
      expect(conditions, testid).not.toBeNull();
      expect(conditions?.filter((c) => SCAN_STATE.test(c)), testid).toEqual([]);
    }
  });
});
