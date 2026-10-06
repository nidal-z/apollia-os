import { beforeEach, describe, expect, test, vi } from "vitest";
import { get } from "svelte/store";

const ipc = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke: ipc }));

import { estimateModelLoad } from "$lib/ipc/llmLoad";
import type { ModelTarget } from "./chatTargets";
import { createPaneLoads } from "./paneLoad";
import type { LoadEstimate } from "$lib/types";

const LLAMA: ModelTarget = {
  kind: "model",
  route: "llama-3.3-70b-Q4_K_M.gguf",
  label: "llama-3.3-70b",
  source: "local",
  format: "gguf",
  server: null,
  parameters: "70B",
  path: "/models/llama-3.3-70b-Q4_K_M.gguf",
};

function estimateOf(verdict: "allowed" | "warned" | "refused"): LoadEstimate {
  return {
    modelId: LLAMA.route,
    engine: "llama-server",
    modelPath: LLAMA.path!,
    configurable: true,
    nCtx: 8192,
    memory: { gpuGb: 40, totalGb: 42.5, capacityGb: 64, fit: "too_large", kvCacheMeasured: false },
    guardrail: { level: "balanced", verdict, budgetGb: 18 },
  };
}

/** The commands sent, in order. */
function commands(): unknown[] {
  return ipc.mock.calls.map((call) => call[0]);
}

beforeEach(() => {
  ipc.mockReset();
});

describe("a pane that would load one model too many", () => {
  test("the refusal of the guardrail shows in that pane only, and nothing is loaded", async () => {
    // GIVEN two panes and a runtime whose guardrail refuses the load
    ipc.mockResolvedValue(estimateOf("refused"));
    const loads = createPaneLoads(estimateModelLoad);

    // WHEN the second pane asks for the model
    const mayOpen = await loads.gate("pane-2", LLAMA);

    // THEN the loading window stays closed, the second pane holds the
    // refusal with its figures, the first pane holds none, and the runtime
    // was asked for an estimate, never for a load
    expect(mayOpen).toBe(false);
    const refusals = get(loads.refusals);
    expect(refusals["pane-2"]).toEqual({
      target: LLAMA,
      refusal: { estimatedGb: 42.5, budgetGb: 18, level: "balanced" },
    });
    expect(refusals["main"]).toBeUndefined();
    expect(Object.keys(refusals)).toEqual(["pane-2"]);
    expect(ipc.mock.calls).toEqual([
      ["estimate_model_load", { request: { modelId: LLAMA.route } }],
    ]);
    expect(commands()).not.toContain("load_model");
  });

  test("a load the guardrail lets through opens the loading window as usual", async () => {
    // GIVEN a runtime whose guardrail allows the load, then warns
    const loads = createPaneLoads(estimateModelLoad);

    for (const verdict of ["allowed", "warned"] as const) {
      ipc.mockResolvedValue(estimateOf(verdict));

      // WHEN a pane asks for the model
      const mayOpen = await loads.gate("pane-2", LLAMA);

      // THEN the window may open and no pane holds a refusal
      expect(mayOpen).toBe(true);
      expect(get(loads.refusals)).toEqual({});
    }
  });

  test("an estimate that fails leaves the loading window to say why", async () => {
    // GIVEN a runtime that cannot estimate
    ipc.mockRejectedValue(new Error("model not found in the index"));
    const loads = createPaneLoads(estimateModelLoad);

    // WHEN a pane asks for the model
    const mayOpen = await loads.gate("pane-2", LLAMA);

    // THEN the window opens and no refusal is invented
    expect(mayOpen).toBe(true);
    expect(get(loads.refusals)).toEqual({});
  });

  test("asking again clears the refusal the pane held", async () => {
    // GIVEN a pane holding a refusal
    ipc.mockResolvedValue(estimateOf("refused"));
    const loads = createPaneLoads(estimateModelLoad);
    await loads.gate("pane-2", LLAMA);

    // WHEN the pane asks for a model the guardrail allows
    ipc.mockResolvedValue(estimateOf("allowed"));
    await loads.gate("pane-2", LLAMA);

    // THEN the pane holds no refusal any more
    expect(get(loads.refusals)).toEqual({});
  });

  test("dismissing the refusal of one pane keeps the refusal of another", async () => {
    // GIVEN two panes each holding a refusal
    ipc.mockResolvedValue(estimateOf("refused"));
    const loads = createPaneLoads(estimateModelLoad);
    await loads.gate("pane-2", LLAMA);
    await loads.gate("pane-3", LLAMA);

    // WHEN the first of them is dismissed, twice
    loads.dismiss("pane-2");
    loads.dismiss("pane-2");

    // THEN the other pane still shows its own
    expect(Object.keys(get(loads.refusals))).toEqual(["pane-3"]);
  });
});
