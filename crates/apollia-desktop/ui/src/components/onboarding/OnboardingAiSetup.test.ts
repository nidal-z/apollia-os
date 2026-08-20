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

// ── A choice already on screen survives the next scan ──────────────────────
//
// Two scan paths reach the voice section, `loadData` and `rescanStt`, and both
// used to overwrite the selected model and the dictation toggle with the first
// result of the scan. Five triggers reach those two functions, four of them
// rendered unconditionally, so a choice could be undone by a button that
// promises to refresh a list. The decision is now a function, and the two
// paths delegate to it; both halves are asserted, since a correct decision
// nothing calls would change nothing.

import { reconcileWhisperScan } from "./OnboardingAiSetup.svelte";

const AST = parse(SOURCE, { modern: true });

/** Top-level functions of the instance script, by name. */
const INSTANCE_FUNCTIONS = new Map<string, Record<string, unknown>>(
  ((AST.instance?.content.body ?? []) as unknown as Record<string, unknown>[])
    .filter((node) => node.type === "FunctionDeclaration")
    .map((node) => [(node.id as { name: string }).name, node]),
);

/** Source text of a top-level function of the instance script, `null` if absent. */
function functionSource(name: string): string | null {
  const node = INSTANCE_FUNCTIONS.get(name) as { start: number; end: number } | undefined;
  return node ? SOURCE.slice(node.start, node.end) : null;
}

/**
 * Source text of the condition of a leading `if (...) return;` guard, `null`
 * when the function does not exist or does not open with one.
 */
function guardExpression(name: string): string | null {
  const node = INSTANCE_FUNCTIONS.get(name) as
    | { body: { body: Record<string, unknown>[] } }
    | undefined;
  const first = node?.body.body[0];
  if (first?.type !== "IfStatement") return null;
  const consequent = first.consequent as { type: string };
  if (consequent.type !== "ReturnStatement") return null;
  const test = first.test as { start: number; end: number };
  return SOURCE.slice(test.start, test.end);
}

/**
 * Source text of one attribute of the element carrying `testid`, `null` when
 * the element has no such attribute, and `null` too when nothing carries the
 * identifier, which the tests separate by asking for the element first.
 */
function attributeSource(testid: string, attribute: string): string | null {
  let hit: string | null = null;
  let seen = false;

  const walk = (node: unknown): void => {
    if (Array.isArray(node)) {
      for (const child of node) walk(child);
      return;
    }
    if (node === null || typeof node !== "object") return;
    const record = node as Record<string, unknown>;
    const attributes = record.attributes;

    if (Array.isArray(attributes)) {
      const identifier = attributes.find(
        (a: Record<string, unknown>) => a.type === "Attribute" && a.name === "data-testid",
      ) as Record<string, unknown> | undefined;
      const value = identifier?.value;
      const first = Array.isArray(value) ? (value[0] as Record<string, unknown>) : null;
      if (first?.type === "Text" && first.data === testid) {
        seen = true;
        const wanted = attributes.find(
          (a: Record<string, unknown>) => a.type === "Attribute" && a.name === attribute,
        ) as { start: number; end: number } | undefined;
        if (wanted) hit = SOURCE.slice(wanted.start, wanted.end);
      }
    }

    for (const key of Object.keys(record)) {
      if (key === "parent") continue;
      walk(record[key]);
    }
  };

  walk(AST.fragment);
  return seen ? hit : null;
}

/** Elements a test asks about must exist before its answer means anything. */
function carriesTestid(testid: string): boolean {
  return conditionsAbove(testid) !== null;
}

describe("OnboardingAiSetup - reconcileWhisperScan", () => {
  const first = { path: "/models/ggml-base.bin", recommended: false };
  const second = { path: "/models/ggml-large-v3-turbo-q5_0.bin", recommended: true };

  test("a scan that still reports the chosen model keeps it, toggle included", () => {
    // GIVEN an operator who picked the second model and turned dictation on,
    // and a re-scan that reports both models again
    const choice = reconcileWhisperScan([first, second], second.path, true);

    // THEN the scan changes neither the selection nor the toggle
    expect(choice.selectedPath).toBe(second.path);
    expect(choice.dictationEnabled).toBe(true);
  });

  test("a toggle left off stays off when the chosen model is still there", () => {
    // GIVEN the same choice with dictation deliberately switched off, on a
    // model whose scan flag says it is the recommended one
    const choice = reconcileWhisperScan([first, second], second.path, false);

    // THEN the re-scan does not switch dictation back on from the flag
    expect(choice.selectedPath).toBe(second.path);
    expect(choice.dictationEnabled).toBe(false);
  });

  test("the first scan of a session proposes the first model and its flag", () => {
    // GIVEN a session where nothing has been chosen yet
    const choice = reconcileWhisperScan([second, first], null, false);

    // THEN the section opens usable, on the first result and its own flag
    expect(choice.selectedPath).toBe(second.path);
    expect(choice.dictationEnabled).toBe(true);
  });

  test("a chosen model the scan no longer reports falls back on the first", () => {
    // GIVEN a selection pointing at a file that has left the disk
    const choice = reconcileWhisperScan([first], "/models/deleted.bin", true);

    // THEN the section falls back rather than pointing at nothing, and the
    // toggle follows the model it fell back on
    expect(choice.selectedPath).toBe(first.path);
    expect(choice.dictationEnabled).toBe(false);
  });

  test("a scan that finds nothing leaves nothing selected", () => {
    // GIVEN a scan run after the last voice model was removed
    const choice = reconcileWhisperScan([], second.path, true);

    // THEN no path is kept, and dictation cannot claim to run
    expect(choice.selectedPath).toBeNull();
    expect(choice.dictationEnabled).toBe(false);
  });
});

describe("OnboardingAiSetup - both scan paths delegate the choice", () => {
  const ASSIGNS_VOICE_CHOICE = /(selectedWhisper|sttEnabled)\s*=[^=]/;

  test("the matcher matches where the assignment is, and the walker reads code", () => {
    // GIVEN the one function that is supposed to assign the voice choice
    const applier = functionSource("applyWhisperScan");

    // THEN it exists, it is where the assignments live, and it decides through
    // the exported function, so the absence asserted below is a measurement
    expect(applier).not.toBeNull();
    expect(ASSIGNS_VOICE_CHOICE.test(applier ?? "")).toBe(true);
    expect(applier).toContain("reconcileWhisperScan(");
    // AND a function that does not exist is reported as absent
    expect(functionSource("noSuchFunction")).toBeNull();
  });

  test("neither scan path writes the voice choice itself", () => {
    // GIVEN the two functions the five triggers of the step reach
    for (const name of ["loadData", "rescanStt"]) {
      const source = functionSource(name);
      // THEN each exists and hands the scan result to the shared applier
      expect(source, name).not.toBeNull();
      expect(source, name).toContain("applyWhisperScan(");
      // AND neither overwrites the selection or the toggle on its own
      expect(ASSIGNS_VOICE_CHOICE.test(source ?? ""), name).toBe(false);
    }
  });

  test("the applier is the only function that writes the voice choice", () => {
    // GIVEN every top-level function of the instance script
    const writers = [...INSTANCE_FUNCTIONS.keys()].filter((name) =>
      ASSIGNS_VOICE_CHOICE.test(functionSource(name) ?? ""),
    );

    // THEN exactly one writes it, so no path can grow its own overwrite again
    expect(writers).toEqual(["applyWhisperScan"]);
  });

  test("the operator keeps the one write that is his", () => {
    // GIVEN the row of a detected voice model
    expect(carriesTestid("whisper-model-row")).toBe(true);

    // THEN clicking it still sets the selection, which is the write the two
    // scan paths were undoing
    expect(attributeSource("whisper-model-row", "onclick")).toContain(
      "selectedWhisper = model",
    );
  });
});

describe("OnboardingAiSetup - the detected engines stay selectable", () => {
  test("the probes read real attributes and real guards", () => {
    // GIVEN two places that legitimately carry what the tests below ask for
    // THEN the attribute probe returns an expression rather than nothing
    expect(carriesTestid("stt-toggle")).toBe(true);
    expect(attributeSource("stt-toggle", "disabled")).toContain("whisperModels.length");
    // AND the guard probe returns the condition of a leading early return
    expect(guardExpression("loadSttModelFile")).toBe("importingStt");
    // AND both report absence when there is nothing to read
    expect(attributeSource("llm-model-row", "no-such-attribute")).toBeNull();
    expect(guardExpression("noSuchFunction")).toBeNull();
  });

  test("a configured session no longer disables the rows it just used", () => {
    // GIVEN the row of an engine found on disk
    expect(carriesTestid("llm-model-row")).toBe(true);
    const disabled = attributeSource("llm-model-row", "disabled");

    // THEN it still declines a click while a configuration is in flight
    expect(disabled).toContain("llmConfiguring");
    // AND it no longer locks itself on the success of this session, which is
    // the answer the two other lists of the step already gave
    expect(disabled).not.toContain("llmSuccess");
    expect(attributeSource("whisper-model-row", "disabled")).toBeNull();
    expect(attributeSource("curated-stt-row", "disabled")).toBeNull();
  });

  test("the click handler no longer refuses a second engine either", () => {
    // GIVEN the handler behind those rows
    const guard = guardExpression("selectGgufModel");

    // THEN it opens on a guard that still ignores a click mid-configuration
    expect(guard).toContain("llmConfiguring");
    // AND that guard no longer refuses the click once one engine is wired
    expect(guard).not.toContain("llmSuccess");
  });
});

// ── The lock a configuration raises comes back down ─────────────────────────
//
// The rows of the detected engines used to be disabled by `llmConfiguring ||
// llmSuccess`, so a first success closed the list for the whole session.
// Dropping the second term changed nothing on its own, because the lock itself
// was never lowered on the path that succeeds: it only came down inside the
// `catch`. The lifecycle is now one exported function, so the state it leaves
// behind can be executed rather than read off the template, and the handler is
// asserted to delegate it: a released lock nothing calls would release nothing.

import {
  runLlmConfiguration,
  type LlmConfigurationState,
} from "./OnboardingAiSetup.svelte";

describe("OnboardingAiSetup - runLlmConfiguration", () => {
  const IDLE: LlmConfigurationState = {
    selectedPath: null,
    configuring: false,
    configured: false,
    error: null,
  };
  const WIRED: LlmConfigurationState = {
    selectedPath: "/models/first.gguf",
    configuring: false,
    configured: true,
    error: null,
  };

  /** Every state one run publishes, in order, copied so later runs cannot edit them. */
  async function statesOf(
    current: LlmConfigurationState,
    path: string,
    wire: (path: string) => Promise<void>,
  ): Promise<LlmConfigurationState[]> {
    const published: LlmConfigurationState[] = [];
    await runLlmConfiguration(current, path, wire, (next) => published.push({ ...next }));
    return published;
  }

  test("a run that succeeds settles with the lock down", async () => {
    // GIVEN an operator who already wired an engine this session and clicks the
    // row of another one, whose configuration goes through
    const published = await statesOf(WIRED, "/models/second.gguf", async () => {});

    // THEN the run raised the lock while it was in flight
    expect(published[0].configuring).toBe(true);
    // AND the state it settled on has it down, which is the whole of clause 2:
    // every row stays clickable once an engine is wired
    expect(published.at(-1)?.configuring).toBe(false);
    // AND the engine that was just wired is the one the step names
    expect(published.at(-1)?.configured).toBe(true);
    expect(published.at(-1)?.selectedPath).toBe("/models/second.gguf");
    expect(published.at(-1)?.error).toBeNull();
  });

  test("the first run of a session settles the same way", async () => {
    // GIVEN a step where nothing has been configured yet
    const published = await statesOf(IDLE, "/models/first.gguf", async () => {});

    // THEN the settled state is complete, lock down included
    expect(published.at(-1)).toEqual({
      selectedPath: "/models/first.gguf",
      configuring: false,
      configured: true,
      error: null,
    });
  });

  test("a run that fails settles with the lock down and the wired engine kept", async () => {
    // GIVEN a click on an engine the runtime refuses
    const published = await statesOf(WIRED, "/models/broken.gguf", async () => {
      throw new Error("model refused");
    });

    // THEN the lock does not survive the failure either
    expect(published.at(-1)?.configuring).toBe(false);
    // AND the step reports the refusal while still naming the engine in use
    expect(published.at(-1)?.error).toBe("model refused");
    expect(published.at(-1)?.selectedPath).toBe(WIRED.selectedPath);
    expect(published.at(-1)?.configured).toBe(true);
  });

  test("the lock stays up as long as the effect has not returned", async () => {
    // GIVEN a configuration whose effect is still running
    let finish: () => void = () => {};
    const effect = new Promise<void>((resolve) => {
      finish = resolve;
    });
    const published: LlmConfigurationState[] = [];
    const run = runLlmConfiguration(IDLE, "/models/first.gguf", () => effect, (next) =>
      published.push({ ...next }),
    );
    await Promise.resolve();

    // THEN one state has been published, and it holds the lock up, so the
    // release asserted above measures the end of the run and not its absence
    expect(published).toHaveLength(1);
    expect(published[0].configuring).toBe(true);

    // WHEN the effect returns
    finish();
    await run;

    // THEN the lock comes down
    expect(published).toHaveLength(2);
    expect(published.at(-1)?.configuring).toBe(false);
  });
});

describe("OnboardingAiSetup - the click handler delegates that lifecycle", () => {
  const SETS_LOCK_TO_LITERAL = /llmConfiguring\s*=\s*(?:true|false)\b/;

  test("the matcher matches how the lock used to be driven", () => {
    // GIVEN the two ways of writing the lock, the old one and the current one
    // THEN the matcher tells them apart, so the absences below are measurements
    expect(SETS_LOCK_TO_LITERAL.test("llmConfiguring = true;")).toBe(true);
    expect(SETS_LOCK_TO_LITERAL.test("      llmConfiguring = false;")).toBe(true);
    expect(SETS_LOCK_TO_LITERAL.test("llmConfiguring = next.configuring;")).toBe(false);
  });

  test("the handler hands the whole run to the exported lifecycle", () => {
    // GIVEN the function behind a row of the detected engines
    const source = functionSource("selectGgufModel");

    // THEN it exists and runs the configuration through the exported function
    expect(source).not.toBeNull();
    expect(source).toContain("runLlmConfiguration(");
    // AND it no longer drives the lock by hand, which is where a raise without
    // its matching release could be written again
    expect(SETS_LOCK_TO_LITERAL.test(source ?? "")).toBe(false);
  });

  test("no function of the step raises the lock on its own", () => {
    // GIVEN every top-level function of the instance script
    const writers = [...INSTANCE_FUNCTIONS.keys()].filter((name) =>
      SETS_LOCK_TO_LITERAL.test(functionSource(name) ?? ""),
    );

    // THEN none of them sets the lock to a literal: the only writes left mirror
    // the state the run publishes, and every run publishes a released lock
    expect(writers).toEqual([]);
  });
});
