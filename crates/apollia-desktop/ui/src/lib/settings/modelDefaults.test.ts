import { describe, it, expect } from "vitest";
import type { ResolvedSetting } from "$lib/ipc/settings";
import {
  BYPASS_CHOICES,
  CONTEXT_CHOICES,
  DEFAULT_GUARDRAIL_LEVEL,
  DEFAULT_LOAD_BYPASS,
  GUARDRAIL_CHOICES,
  MODEL_DEFAULT_BLOCKS,
  MODEL_DEFAULT_KEYS,
  UNLOAD_CHOICES,
  blockLock,
  imageSideLimit,
  initialModelDefaults,
  loadAnywayNeedsConfirmation,
  loadModelDefaults,
  readField,
  readImageSideLimit,
  unloadMode,
  writeModelDefault,
  type ModelDefaultsIpc,
} from "./modelDefaults";

/** A shell holding `stored`, where the keys of `imposed` are fixed by a test policy. */
function shell(
  stored: Record<string, unknown> = {},
  imposed: Record<string, unknown> = {},
): ModelDefaultsIpc & { written: [string, unknown][] } {
  const written: [string, unknown][] = [];
  const resolve = (key: string): ResolvedSetting => {
    if (key in imposed) return { key, value: imposed[key], source: "policy" };
    if (key in stored) return { key, value: stored[key], source: "user" };
    throw new Error(`no stored value for ${key}`);
  };
  return {
    written,
    read: async (key) => resolve(key),
    write: async (key, value) => {
      if (key in imposed) throw new Error("locked by the organization policy");
      written.push([key, value]);
      stored[key] = value;
      return resolve(key);
    },
  };
}

describe("the blocks of Model defaults", () => {
  it("follow the order of the reference screen", () => {
    // GIVEN the screen
    // WHEN its blocks are listed
    // THEN images come first, then the context, the guardrails, the bypass,
    //      and the unload delay last
    expect([...MODEL_DEFAULT_BLOCKS]).toEqual([
      "images",
      "context",
      "guardrails",
      "bypass",
      "unload",
    ]);
  });

  it("offer two context choices, the custom value first", () => {
    // GIVEN the context block
    // WHEN its choices are listed
    // THEN they are the custom value, then the model maximum
    expect(CONTEXT_CHOICES.map((c) => c.value)).toEqual(["custom", "model_max"]);
  });
});

describe("the guardrail selector", () => {
  it("offers the five levels, balanced by default", async () => {
    // GIVEN a machine nobody configured
    const state = await loadModelDefaults(shell());

    // WHEN the levels and the level in force are read
    const levels = GUARDRAIL_CHOICES.map((c) => c.value);

    // THEN there are five, in the order of the screen, and balanced is chosen
    expect(levels).toEqual(["disabled", "lenient", "balanced", "strict", "ceiling"]);
    expect(state.values.guardrailLevel).toBe(DEFAULT_GUARDRAIL_LEVEL);
    expect(DEFAULT_GUARDRAIL_LEVEL).toBe("balanced");
  });

  it("writes the level under the key the load check reads", async () => {
    // GIVEN a shell and the default state
    const ipc = shell();
    const state = initialModelDefaults();

    // WHEN strict is chosen
    const outcome = await writeModelDefault(state, "guardrailLevel", "strict", ipc);

    // THEN it is written under llm.guardrails.level and shown from the user
    expect(ipc.written).toEqual([["llm.guardrails.level", "strict"]]);
    expect(outcome.written).toBe(true);
    expect(outcome.state.values.guardrailLevel).toBe("strict");
    expect(outcome.state.sources.guardrailLevel).toBe("user");
  });

  it("is disabled and names the policy when a policy fixes the guardrails", async () => {
    // GIVEN a test policy that fixes the level at strict
    const ipc = shell({}, { [MODEL_DEFAULT_KEYS.guardrailLevel]: "strict" });
    const state = await loadModelDefaults(ipc);

    // WHEN the lock of each block is read, and the user tries another level
    const guardrails = blockLock("guardrails", state.sources);
    const context = blockLock("context", state.sources);
    const outcome = await writeModelDefault(state, "guardrailLevel", "disabled", ipc);

    // THEN the selector is disabled under "Enforced by policy", the imposed
    //      level stays shown, nothing is written, and the other blocks are free
    expect(guardrails).toEqual({ disabled: true, policyLabelKey: "settings.policy_locked" });
    expect(state.values.guardrailLevel).toBe("strict");
    expect(outcome.written).toBe(false);
    expect(ipc.written).toEqual([]);
    expect(context).toEqual({ disabled: false, policyLabelKey: null });
  });

  it("refuses a ceiling above the memory of the machine", () => {
    // GIVEN a machine that holds 16 GB
    // WHEN 24 GB, 12 GB and a word are typed as the ceiling
    const above = readField("guardrailCeiling", "24", 16);
    const within = readField("guardrailCeiling", "12,5", 16);
    const word = readField("guardrailCeiling", "plenty", 16);

    // THEN the first is refused against the memory, the second is read, and
    //      the third is no number
    expect(above).toMatchObject({ ok: false, fault: "above_memory", max: 16 });
    expect(within).toEqual({ ok: true, value: 12.5 });
    expect(word).toMatchObject({ ok: false, fault: "not_a_number" });
  });
});

describe("the bypass of a memory warning", () => {
  it("asks for an explicit confirmation by default", async () => {
    // GIVEN a machine nobody configured
    const state = await loadModelDefaults(shell());

    // WHEN the choices and the one in force are read
    // THEN the explicit confirmation is first and chosen, and "load anyway"
    //      asks for it
    expect(BYPASS_CHOICES.map((c) => c.value)).toEqual(["confirm", "unrestricted"]);
    expect(state.values.bypass).toBe(DEFAULT_LOAD_BYPASS);
    expect(loadAnywayNeedsConfirmation(state.values.bypass)).toBe(true);
  });

  it("skips the confirmation only under no restriction", async () => {
    // GIVEN the default state
    const ipc = shell();

    // WHEN no restriction is chosen
    const outcome = await writeModelDefault(initialModelDefaults(), "bypass", "unrestricted", ipc);

    // THEN it is written, and "load anyway" no longer asks
    expect(ipc.written).toEqual([["llm.guardrails.bypass", "unrestricted"]]);
    expect(loadAnywayNeedsConfirmation(outcome.state.values.bypass)).toBe(false);
  });

  it("is locked when a policy fixes it", async () => {
    // GIVEN a test policy that holds the bypass to the confirmation
    const state = await loadModelDefaults(shell({}, { [MODEL_DEFAULT_KEYS.bypass]: "confirm" }));

    // WHEN its lock is read
    // THEN the block is disabled under the policy
    expect(blockLock("bypass", state.sources).disabled).toBe(true);
  });
});

describe("the unload delay", () => {
  it("starts at sixty minutes and writes under llm.residency", async () => {
    // GIVEN a machine nobody configured
    const ipc = shell();
    const state = await loadModelDefaults(ipc);

    // WHEN one minute is typed and written
    const typed = readField("idleUnload", "1");
    const outcome = await writeModelDefault(state, "idleUnload", typed.ok ? typed.value : -1, ipc);

    // THEN the start value was sixty, and one minute is written under the key
    //      the sweep reads
    expect(state.values.idleUnload).toBe(60);
    expect(unloadMode(state.values.idleUnload)).toBe("after");
    expect(ipc.written).toEqual([["llm.residency.idle_unload_minutes", 1]]);
    expect(outcome.state.values.idleUnload).toBe(1);
  });

  it("reads zero as never", async () => {
    // GIVEN a file where the delay is zero
    const state = await loadModelDefaults(shell({ [MODEL_DEFAULT_KEYS.idleUnload]: 0 }));

    // WHEN the choice shown is read
    // THEN it is "never", the second of the two choices
    expect(unloadMode(state.values.idleUnload)).toBe("never");
    expect(UNLOAD_CHOICES.map((c) => c.value)).toEqual(["after", "never"]);
  });

  it("is disabled and names the policy when a policy fixes it", async () => {
    // GIVEN a test policy that fixes the delay at fifteen minutes
    const ipc = shell({}, { [MODEL_DEFAULT_KEYS.idleUnload]: 15 });
    const state = await loadModelDefaults(ipc);

    // WHEN its lock is read and the user asks for never
    const lock = blockLock("unload", state.sources);
    const outcome = await writeModelDefault(state, "idleUnload", 0, ipc);

    // THEN the field is disabled under "Enforced by policy" and keeps fifteen
    expect(lock).toEqual({ disabled: true, policyLabelKey: "settings.policy_locked" });
    expect(outcome.state.values.idleUnload).toBe(15);
    expect(ipc.written).toEqual([]);
  });

  it("refuses a delay that is not a whole number of minutes", () => {
    // GIVEN the field of the delay
    // WHEN a fraction and zero are typed
    // THEN neither is read as a delay
    expect(readField("idleUnload", "1.5")).toMatchObject({ ok: false, fault: "not_a_number" });
    expect(readField("idleUnload", "0")).toMatchObject({ ok: false, fault: "out_of_bounds" });
  });
});

describe("the ceiling on input images", () => {
  it("is on at 2048 pixels by default", async () => {
    // GIVEN a machine nobody configured, and a shell that does not answer
    const state = await loadModelDefaults(shell());
    const silent = await readImageSideLimit({
      read: async () => {
        throw new Error("shell unreachable");
      },
      write: async () => {
        throw new Error("shell unreachable");
      },
    });

    // WHEN the ceiling is read
    // THEN it is 2048 either way
    expect(imageSideLimit(state.values)).toBe(2048);
    expect(silent).toBe(2048);
  });

  it("is lifted when the switch is off, and follows the figure written", async () => {
    // GIVEN a file where the switch is off, then one at 1024 pixels
    const off = await readImageSideLimit(shell({ [MODEL_DEFAULT_KEYS.imageLimit]: false }));
    const smaller = await readImageSideLimit(shell({ [MODEL_DEFAULT_KEYS.imageMaxSide]: 1024 }));

    // WHEN the ceiling the composer applies is read
    // THEN no ceiling in the first case, 1024 in the second
    expect(off).toBeNull();
    expect(smaller).toBe(1024);
  });

  it("reads the figure typed in its field within its bounds", () => {
    // GIVEN the field of the largest side
    // WHEN a figure with a thin space, a figure too small and nothing are typed
    // THEN the first is read, the others are refused with the bounds
    expect(readField("imageMaxSide", "2 048")).toEqual({ ok: true, value: 2048 });
    expect(readField("imageMaxSide", "12")).toMatchObject({ ok: false, fault: "out_of_bounds", min: 256 });
    expect(readField("imageMaxSide", "")).toMatchObject({ ok: false, fault: "not_a_number" });
  });

  it("is locked when a policy fixes it", async () => {
    // GIVEN a test policy that fixes the largest side
    const state = await loadModelDefaults(shell({}, { [MODEL_DEFAULT_KEYS.imageMaxSide]: 1024 }));

    // WHEN the lock of the block is read
    // THEN it is disabled, and the imposed figure is the one shown
    expect(blockLock("images", state.sources).disabled).toBe(true);
    expect(state.values.imageMaxSide).toBe(1024);
  });
});

describe("the default context", () => {
  it("is a custom value left to Apollia until a figure is written", async () => {
    // GIVEN a machine nobody configured
    const ipc = shell();
    const state = await loadModelDefaults(ipc);

    // WHEN 16384 tokens are typed and written
    const typed = readField("contextTokens", "16384");
    const outcome = await writeModelDefault(state, "contextTokens", typed.ok ? typed.value : -1, ipc);

    // THEN the start was the custom value with no figure, and the figure is
    //      written under llm.defaults
    expect(state.values.contextMode).toBe("custom");
    expect(state.values.contextTokens).toBe(0);
    expect(ipc.written).toEqual([["llm.defaults.context_tokens", 16384]]);
    expect(outcome.state.values.contextTokens).toBe(16384);
  });

  it("refuses a window outside its bounds", () => {
    // GIVEN the field of the custom value
    // WHEN 300000 tokens are typed
    // THEN the figure is refused with the bounds it must stay within
    expect(readField("contextTokens", "300000")).toEqual({
      ok: false,
      fault: "out_of_bounds",
      min: 512,
      max: 262_144,
    });
  });

  it("keeps the state when the shell refuses a write", async () => {
    // GIVEN a shell whose writes fail
    const ipc: ModelDefaultsIpc = {
      read: async (key) => ({ key, value: null, source: "default" }),
      write: async () => {
        throw new Error("unreadable file");
      },
    };
    const state = initialModelDefaults();

    // WHEN the model maximum is chosen
    const outcome = await writeModelDefault(state, "contextMode", "model_max", ipc);

    // THEN the choice shown does not move
    expect(outcome.written).toBe(false);
    expect(outcome.state.values.contextMode).toBe("custom");
  });
});
