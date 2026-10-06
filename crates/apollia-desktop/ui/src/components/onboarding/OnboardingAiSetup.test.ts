import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, test, expect } from "vitest";
import { parse } from "svelte/compiler";
import {
  LANGUAGE_DATA_EXTENSION,
  canContinueAiStep,
  isLanguageData,
  llmSectionView,
} from "./aiSetupRules";

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

// ── The template hangs off the decision, not off the raw scan state ────────
//
// The block above tests a function. What closed the door on the operator was
// the markup: the add means lived inside `{#if ggufModels.length === 0 &&
// !llmSuccess}`. So the template is parsed and each region is asked which
// conditions stand above it.

/**
 * One probe per component of the step.
 *
 * The step is two files: the shell and the language-engine section. Every
 * assertion below names the file that carries the code it is
 * about, so a region moved between them fails here rather than passing on an
 * empty read.
 */
function probe(file: string) {
  const source = readFileSync(fileURLToPath(new URL(file, import.meta.url)), "utf8");
  const ast = parse(source, { modern: true });

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
        inner = [...stack, source.slice(condition.start, condition.end)];
      }

      for (const key of Object.keys(record)) {
        if (key === "parent") continue;
        walk(record[key], inner);
      }
    };

    walk(ast.fragment, []);
    return hit;
  }

  /** Top-level functions of the instance script, by name. */
  const functions = new Map<string, Record<string, unknown>>(
    ((ast.instance?.content.body ?? []) as unknown as Record<string, unknown>[])
      .filter((node) => node.type === "FunctionDeclaration")
      .map((node) => [(node.id as { name: string }).name, node]),
  );

  /** Source text of a top-level function of the instance script, `null` if absent. */
  function functionSource(name: string): string | null {
    const node = functions.get(name) as { start: number; end: number } | undefined;
    return node ? source.slice(node.start, node.end) : null;
  }

  /**
   * Source text of the condition of a leading `if (...) return;` guard, `null`
   * when the function does not exist or does not open with one.
   */
  function guardExpression(name: string): string | null {
    const node = functions.get(name) as
      | { body: { body: Record<string, unknown>[] } }
      | undefined;
    const first = node?.body.body[0];
    if (first?.type !== "IfStatement") return null;
    const consequent = first.consequent as { type: string };
    if (consequent.type !== "ReturnStatement") return null;
    const test = first.test as { start: number; end: number };
    return source.slice(test.start, test.end);
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
          if (wanted) hit = source.slice(wanted.start, wanted.end);
        }
      }

      for (const key of Object.keys(record)) {
        if (key === "parent") continue;
        walk(record[key]);
      }
    };

    walk(ast.fragment);
    return seen ? hit : null;
  }

  /** Elements a test asks about must exist before its answer means anything. */
  const carriesTestid = (testid: string): boolean => conditionsAbove(testid) !== null;

  return {
    functionNames: () => [...functions.keys()],
    conditionsAbove,
    functionSource,
    guardExpression,
    attributeSource,
    carriesTestid,
  };
}

const LLM = probe("./OnboardingLlmSection.svelte");
const SHELL = probe("./OnboardingAiSetup.svelte");

/** Reactive state that must never gate a list or a way of adding a model. */
const SCAN_STATE = /ggufModels|llmSuccess/;

describe("OnboardingAiSetup - template branches", () => {
  test("the walker reads real conditions, and the matcher matches the old ones", () => {
    // GIVEN a region that legitimately keeps a condition
    // THEN the walker returns its condition source, so an empty answer below
    // cannot be an artefact of a walker that finds nothing
    expect(LLM.conditionsAbove("llm-download-progress")).toContain("llmDownloading");

    // AND the matcher does match the condition that used to gate the add means
    expect(SCAN_STATE.test("ggufModels.length === 0 && !llmSuccess")).toBe(true);

    // AND an absent region is reported as absent, not as unconditioned
    expect(LLM.conditionsAbove("no-such-testid")).toBeNull();
  });

  test("no way of adding a model is gated by what the scan found", () => {
    // GIVEN the three language-engine add means
    for (const [where, testid] of [
      [LLM, "llm-load-model-btn"],
      [LLM, "curated-llm-list"],
      [LLM, "search-results"],
    ] as const) {
      const conditions = where.conditionsAbove(testid);
      // THEN the region exists
      expect(conditions, testid).not.toBeNull();
      // AND nothing above it depends on the scan
      expect(conditions?.filter((c) => SCAN_STATE.test(c)), testid).toEqual([]);
    }
  });

  test("the detected list is not gated by the scan state", () => {
    // GIVEN the list of models found on disk
    const conditions = LLM.conditionsAbove("llm-model-list");

    // THEN it renders off its own decision flag only
    expect(conditions).not.toBeNull();
    expect(conditions?.filter((c) => SCAN_STATE.test(c))).toEqual([]);
  });
});

describe("OnboardingAiSetup - the detected engines stay selectable", () => {
  test("the probes read real attributes and real guards", () => {
    // GIVEN two places that legitimately carry what the tests below ask for
    // THEN the attribute probe returns an expression rather than nothing
    expect(LLM.carriesTestid("llm-load-model-btn")).toBe(true);
    expect(LLM.attributeSource("llm-load-model-btn", "disabled")).toContain("importingLlm");
    // AND the guard probe returns the condition of a leading early return
    expect(LLM.guardExpression("loadLlmModel")).toBe("importingLlm || llmDownloading");
    // AND both report absence when there is nothing to read
    expect(LLM.attributeSource("llm-model-row", "no-such-attribute")).toBeNull();
    expect(LLM.guardExpression("noSuchFunction")).toBeNull();
  });

  test("a configured session no longer disables the rows it just used", () => {
    // GIVEN the row of an engine found on disk
    expect(LLM.carriesTestid("llm-model-row")).toBe(true);
    const disabled = LLM.attributeSource("llm-model-row", "disabled");

    // THEN it still declines a click while a configuration is in flight
    expect(disabled).toContain("llmConfiguring");
    // AND it no longer locks itself on the success of this session
    expect(disabled).not.toContain("llmSuccess");
  });

  test("the click handler no longer refuses a second engine either", () => {
    // GIVEN the handler behind those rows
    const guard = LLM.guardExpression("selectGgufModel");

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
} from "./aiSetupRules";

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
    const source = LLM.functionSource("selectGgufModel");

    // THEN it exists and runs the configuration through the exported function
    expect(source).not.toBeNull();
    expect(source).toContain("runLlmConfiguration(");
    // AND it no longer drives the lock by hand, which is where a raise without
    // its matching release could be written again
    expect(SETS_LOCK_TO_LITERAL.test(source ?? "")).toBe(false);
  });

  test("no function of the step raises the lock on its own", () => {
    // GIVEN every top-level function of the instance script
    const writers = LLM.functionNames().filter((name) =>
      SETS_LOCK_TO_LITERAL.test(LLM.functionSource(name) ?? ""),
    );

    // THEN none of them sets the lock to a literal: the only writes left mirror
    // the state the run publishes, and every run publishes a released lock
    expect(writers).toEqual([]);
  });
});

describe("OnboardingAiSetup - recognition language data", () => {
  test("the import from disk offers the language data beside the model files", () => {
    // GIVEN the function behind the "load a model" button, as written
    const source = LLM.functionSource("loadLlmModel") ?? "";

    // THEN its picker lists the GGUF files and the language data, and the
    // button that opens it is one of the add means that never disappear
    expect(source).toMatch(/extensions: \["gguf"\]/);
    expect(source).toMatch(/extensions: \[LANGUAGE_DATA_EXTENSION\]/);
    expect(LANGUAGE_DATA_EXTENSION).toBe("traineddata");
    expect(LLM.conditionsAbove("llm-load-model-btn")).toEqual(["llmView.showAddMeans"]);
  });

  test("an imported language data file is confirmed, never wired as an engine", () => {
    // GIVEN the destinations the import answers, on either path separator
    const data = ["/home/u/.apollia/models/tessdata/fra.traineddata", "C:\\models\\tessdata\\ENG.TRAINEDDATA"];
    const models = ["/home/u/.apollia/models/phi.gguf", "/models/traineddata/phi.gguf"];

    // THEN only the language data is told apart from a model
    expect(data.map(isLanguageData)).toEqual([true, true]);
    expect(models.map(isLanguageData)).toEqual([false, false]);
    // AND the function returns before the scan and the wiring of an engine,
    // the confirmation hanging off what it stored
    const source = LLM.functionSource("loadLlmModel") ?? "";
    expect(source.indexOf("isLanguageData(dest)")).toBeGreaterThan(-1);
    expect(source.indexOf("isLanguageData(dest)")).toBeLessThan(source.indexOf("selectGgufModel"));
    expect(LLM.conditionsAbove("llm-language-data-imported")).toEqual(["languageDataImported"]);
  });
});

// ── The only step that cannot be passed ────────────────────────────────────
//
// The AI step is the one mandatory step of the first launch. Two halves hold
// it: the rule that opens "Continue", and a template that offers no way round
// it. The same template no longer asks for a context window or for dictation,
// which are settings and not questions of a first launch.

describe("OnboardingAiSetup - the step cannot be passed", () => {
  const NOTHING = { probing: false, engineWired: false, backendCount: 0, downloadInFlight: false };

  test("without a model or a server, continue stays closed", () => {
    // GIVEN a machine with no engine wired, no backend and no download
    // WHEN the step asks whether it may be left
    // THEN it may not
    expect(canContinueAiStep(NOTHING)).toBe(false);
  });

  test("a wired model, a registered server, or a launched download opens it", () => {
    // GIVEN each of the three things that give the agents something to answer with
    // WHEN the step asks whether it may be left
    // THEN each one is enough on its own
    expect(canContinueAiStep({ ...NOTHING, engineWired: true })).toBe(true);
    expect(canContinueAiStep({ ...NOTHING, backendCount: 1 })).toBe(true);
    expect(canContinueAiStep({ ...NOTHING, downloadInFlight: true })).toBe(true);
  });

  test("nothing opens it while the machine is still being probed", () => {
    // GIVEN a backend already registered, and a probe that has not answered
    // WHEN the step asks whether it may be left
    // THEN it waits for the section to be on screen
    expect(canContinueAiStep({ ...NOTHING, probing: true, backendCount: 1 })).toBe(false);
  });

  test("continue is disabled by that rule, and the step offers no skip", () => {
    // GIVEN the template of the step
    // WHEN its continue button and its controls are read
    // THEN the button hangs off the rule
    expect(SHELL.carriesTestid("ai-setup-continue")).toBe(true);
    expect(SHELL.attributeSource("ai-setup-continue", "disabled")).toContain("!canContinue");
    // AND no control of the step or of its section passes it
    for (const where of [SHELL, LLM]) {
      expect(where.carriesTestid("ai-setup-skip")).toBe(false);
    }
    const shell = readFileSync(
      fileURLToPath(new URL("./OnboardingAiSetup.svelte", import.meta.url)),
      "utf8",
    );
    expect(shell).not.toMatch(/onskip/);
  });

  test("the step asks neither for a context window nor for dictation", () => {
    // GIVEN the sources of the step and of its section
    const sources = ["./OnboardingAiSetup.svelte", "./OnboardingLlmSection.svelte"].map((file) =>
      readFileSync(fileURLToPath(new URL(file, import.meta.url)), "utf8"),
    );

    // WHEN the controls that left the first launch are looked for
    // THEN the context window selector is gone, and no window is passed along
    expect(LLM.carriesTestid("llm-context-window")).toBe(false);
    for (const source of sources) {
      expect(source).not.toMatch(/contextWindow|CONTEXT_WINDOW|n_ctx/);
      // AND the dictation section is gone with everything it wired
      expect(source).not.toMatch(/OnboardingSttSection|setupWhisperModel|stt-section/);
    }
    expect(SHELL.carriesTestid("stt-section")).toBe(false);
  });

  test("the engine is wired at the default window of the model", () => {
    // GIVEN the function of the section that wires an engine
    const source = LLM.functionSource("selectGgufModel");

    // WHEN its call to the setup command is read
    // THEN it passes the path alone
    expect(source).toContain("setupLocalLlm(path)");
  });
});
