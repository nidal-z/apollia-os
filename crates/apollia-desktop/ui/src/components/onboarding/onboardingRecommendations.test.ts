import { describe, expect, it } from "vitest";

import type { HardwareProfileView, RecommendedModel } from "$lib/ipc/models";
import {
  caveatLabel,
  defaultChoice,
  measuredLabel,
  memoryLabel,
  modelDisplayName,
  needsOffloadWarning,
  primaryReason,
  reasonLabel,
  rowCaveats,
  sizeLabel,
  supportsToolCalling,
} from "./onboardingRecommendations";

const GIB = 1024 * 1024 * 1024;

function model(overrides: Partial<RecommendedModel> = {}): RecommendedModel {
  return {
    file: {
      repo_id: "Qwen/Qwen3-14B-GGUF",
      filename: "Qwen3-14B-Q4_K_M.gguf",
      download_url: "https://huggingface.co/Qwen/Qwen3-14B-GGUF/resolve/main/x.gguf",
      size_bytes: Math.round(8.4 * GIB),
      gated: false,
    },
    family_id: "qwen3",
    family_label: "Qwen3",
    params_b: 14,
    quant: "Q4_K_M",
    verdict: { status: "supported" },
    estimate: {
      weights_gb: 8.4,
      kv_cache_gb: 5.0,
      overhead_gb: 0.42,
      total_gb: 13.82,
      kv_cache_measured: true,
      layout: {
        kind: "dense",
        full_layers: 40,
        window_layers: 0,
        recurrent_layers: 0,
        full_cells: 32768,
        window_cells: 0,
        bytes: 5368709120,
      },
    },
    offload: {
      n_gpu_layers: 40,
      total_layers: 40,
      device_gb: 13.82,
      host_gb: 0,
      fully_offloaded: true,
      tokens_per_second: 45,
    },
    badge: "might_fit",
    max_context: null,
    score: 73,
    reasons: [],
    ...overrides,
  };
}

describe("modelDisplayName", () => {
  it("names the model rather than the quantiser who republished it", () => {
    // GIVEN a model whose file name leads with the quantiser's prefix
    const m = model({ file: { ...model().file, filename: "bartowski_Qwen_Qwen3-14B-Q4_K_M.gguf" } });

    // WHEN the row label is built
    const label = modelDisplayName(m);

    // THEN it reads as the family and size, not as the file on disk
    expect(label).toBe("Qwen3 14B");
  });

  it("keeps a fractional size class readable", () => {
    // GIVEN a size class that is not a whole number of billions
    const m = model({ family_label: "Gemma 3", params_b: 1.5 });

    // WHEN the row label is built
    // THEN one decimal is kept rather than the number being rounded away
    expect(modelDisplayName(m)).toBe("Gemma 3 1.5B");
  });
});

describe("sizeLabel", () => {
  it("keeps a decimal below ten gigabytes and drops it above", () => {
    // GIVEN sizes either side of the threshold
    // WHEN each is labelled
    // THEN small sizes keep the precision that distinguishes them
    expect(sizeLabel(2.5 * GIB)).toBe("2.5 GB");
    expect(sizeLabel(18.6 * GIB)).toBe("19 GB");
  });
});

describe("memoryLabel", () => {
  it("names the cache separately so a tight fit is explicable", () => {
    // GIVEN a model whose cache is most of the difference from its file size
    const m = model();

    // WHEN the memory line is built
    const label = memoryLabel(m);

    // THEN the total, the weights, the cache and the engine's own buffers are
    // all interpolated, because the download size alone cannot explain why the
    // fit is tight, and the parts have to add up to the total shown
    expect(label.key).toBe("onboarding.ai_setup.memory_breakdown");
    expect(label.values).toEqual({
      total: "13.8",
      weights: "8.4",
      cache: "5.0",
      overhead: "0.4",
    });
  });

  it("marks an unmeasured cache as an approximation", () => {
    // GIVEN a model whose header did not carry the hyperparameters
    const m = model({ estimate: { ...model().estimate, kv_cache_measured: false } });

    // WHEN the memory line is built
    // THEN a different key is used, so the figure is not shown with the
    // authority of a measurement
    expect(memoryLabel(m).key).toBe("onboarding.ai_setup.memory_breakdown_approx");
  });
});

describe("reasonLabel", () => {
  it("maps every reason the backend can send", () => {
    // GIVEN one of each reason variant
    const reasons: Parameters<typeof reasonLabel>[0][] = [
      { reason: "fits_comfortably", needs_gb: 3.2, budget_gb: 24, pool: "gpu" },
      { reason: "fits_comfortably", needs_gb: 3.2, budget_gb: 48, pool: "unified" },
      { reason: "tight", needs_gb: 13.8, budget_gb: 16, pool: "system" },
      { reason: "split_across_memory", gpu_gb: 15.1, vram_gb: 16, system_gb: 3.2 },
      { reason: "native_tool_calling" },
      { reason: "supersedes_generation", replaces: "Qwen2.5" },
      { reason: "trained_context", tokens: 8192, asked_tokens: 65536 },
      { reason: "reduced_context", tokens: 20480, default_tokens: 32768 },
      { reason: "fully_accelerated", layers: 40 },
      { reason: "partial_offload", gpu_layers: 21, total_layers: 48 },
      { reason: "sparse_mixture", active_percent: 16 },
      { reason: "quantisation", format: "Q4_K_M", retained_percent: 99 },
      { reason: "caveat", caveat: { kind: "no_chat_template" } },
    ];

    // WHEN each is localised
    // THEN every one yields a key, so no reason renders as a blank row
    for (const reason of reasons) {
      expect(reasonLabel(reason).key).toMatch(/^onboarding\.ai_setup\./);
    }
  });

  it("reports a reduced context in thousands of tokens", () => {
    // GIVEN a model that fits only below the runtime's default window
    // WHEN the reason is localised
    const label = reasonLabel({
      reason: "reduced_context",
      tokens: 20480,
      default_tokens: 32768,
    });

    // THEN the figures are the round numbers an operator recognises
    expect(label.values).toEqual({ tokens: 20, defaultTokens: 32 });
  });
});

describe("caveatLabel", () => {
  it("maps every caveat the backend can send", () => {
    // GIVEN one of each caveat variant
    const caveats: Parameters<typeof caveatLabel>[0][] = [
      { kind: "no_chat_template" },
      { kind: "unverified_architecture", architecture: "brandnew5", llama_cpp_tag: "b10092" },
      { kind: "header_truncated", missing: ["tokenizer.chat_template"] },
      { kind: "no_pre_tokenizer" },
    ];

    // WHEN each is localised
    // THEN every one yields a key
    for (const caveat of caveats) {
      expect(caveatLabel(caveat).key).toMatch(/^onboarding\.ai_setup\.caveat_/);
    }
  });
});

describe("primaryReason and rowCaveats", () => {
  it("keeps a caution out of the line that sells the model", () => {
    // GIVEN a model carrying both a positive reason and a caveat
    const m = model({
      reasons: [
        { reason: "caveat", caveat: { kind: "no_chat_template" } },
        { reason: "native_tool_calling" },
      ],
    });

    // WHEN the row's two lines are built
    const primary = primaryReason(m);
    const caveats = rowCaveats(m);

    // THEN the positive one leads and the caution is separate, because a
    // warning styled like a selling point reads as a selling point
    expect(primary?.key).toBe("onboarding.ai_setup.reason_native_tools");
    expect(caveats).toHaveLength(1);
    expect(caveats[0].key).toBe("onboarding.ai_setup.caveat_no_chat_template");
  });

  it("returns no primary reason when a model has only cautions", () => {
    // GIVEN a model whose only reason is a caveat
    const m = model({
      reasons: [{ reason: "caveat", caveat: { kind: "no_pre_tokenizer" } }],
    });

    // WHEN the primary line is built
    // THEN there is none, rather than a caution promoted into the selling line
    expect(primaryReason(m)).toBeNull();
  });
});

describe("supportsToolCalling", () => {
  it("mirrors the backend rule across all three verdicts", () => {
    // GIVEN a supported model, one missing its template, and a rejected one
    const supported = model({ verdict: { status: "supported" } });
    const noTemplate = model({
      verdict: { status: "caveats", caveats: [{ kind: "no_chat_template" }] },
    });
    const otherCaveat = model({
      verdict: { status: "caveats", caveats: [{ kind: "no_pre_tokenizer" }] },
    });
    const rejected = model({
      verdict: { status: "rejected", blockers: [{ kind: "not_gguf", detail: "x" }], caveats: [] },
    });

    // WHEN each is asked whether it drives tools
    // THEN only the missing template withdraws it among the offerable ones
    expect(supportsToolCalling(supported)).toBe(true);
    expect(supportsToolCalling(otherCaveat)).toBe(true);
    expect(supportsToolCalling(noTemplate)).toBe(false);
    expect(supportsToolCalling(rejected)).toBe(false);
  });
});

describe("defaultChoice", () => {
  it("preselects the head of a list the backend already ranked", () => {
    // GIVEN a ranked list
    const best = model({ family_label: "Qwen3", params_b: 14 });
    const second = model({ family_label: "Qwen3", params_b: 8 });

    // WHEN the default is chosen
    // THEN it is the head, and an empty list yields nothing rather than throwing
    expect(defaultChoice([best, second])).toBe(best);
    expect(defaultChoice([])).toBeNull();
  });
});

describe("needsOffloadWarning", () => {
  it("warns only when part of the model will run on the processor", () => {
    // GIVEN a fully offloaded model, a split one, and a processor-only one
    const whole = model();
    const split = model({
      offload: { ...model().offload, n_gpu_layers: 21, total_layers: 48, fully_offloaded: false },
    });
    const cpu = model({
      offload: { ...model().offload, n_gpu_layers: 0, fully_offloaded: false },
    });

    // WHEN each is asked
    // THEN only the split one warns: the other two have nothing the operator
    // could act on by choosing a smaller model
    expect(needsOffloadWarning(whole)).toBe(false);
    expect(needsOffloadWarning(split)).toBe(true);
    expect(needsOffloadWarning(cpu)).toBe(false);
  });
});

describe("quantisation and placement reasons", () => {
  it("interpolates the format and what it retains", () => {
    // GIVEN a quantisation reason
    // WHEN it is localised
    const label = reasonLabel({
      reason: "quantisation",
      format: "Q4_K_M",
      retained_percent: 99,
    });

    // THEN both the name and the retention reach the sentence, since the row
    // has to justify why a compressed model is still the right choice
    expect(label.values).toEqual({ format: "Q4_K_M", retained: 99 });
  });

  it("reports a split placement as layers rather than as a fraction", () => {
    // GIVEN a partially offloaded model
    // WHEN the reason is localised
    const label = reasonLabel({
      reason: "partial_offload",
      gpu_layers: 21,
      total_layers: 48,
    });

    // THEN the counts are carried through, which is what an operator can
    // compare against a smaller model that would fit entirely
    expect(label.values).toEqual({ gpu: 21, total: 48 });
  });
});

describe("measuredLabel", () => {
  const hardware = (accelerator: HardwareProfileView["accelerator"]): HardwareProfileView => ({
    total_ram_gb: 63.9,
    available_ram_gb: 40,
    cpu_model: "AMD Ryzen 9 5950X",
    cpu_cores: 16,
    memory_budget_gb: 16,
    accelerator,
  });

  it("names system memory and video memory separately on a discrete card", () => {
    // GIVEN a desktop with 64 GB of RAM and a 16 GB card
    const label = measuredLabel(
      hardware({ kind: "cuda", device_name: "RTX 4080", vram_gb: 16 }),
    );

    // WHEN the line is built
    // THEN both memories appear with their own figure, never a single
    // "usable" number that reads as the whole machine
    expect(label.key).toBe("onboarding.ai_setup.recommend_measured_on_gpu");
    expect(label.values).toEqual({
      cpu: "AMD Ryzen 9 5950X",
      ram: "64",
      gpu: "RTX 4080",
      vram: "16",
    });
  });

  it("speaks of unified memory on Apple Silicon and of RAM alone without a card", () => {
    // GIVEN a Mac and a processor-only machine
    const mac = measuredLabel(hardware({ kind: "apple_silicon", chip: "M4 Max", vram_gb: 64 }));
    const cpu = measuredLabel(hardware({ kind: "none" }));

    // WHEN the lines are built
    // THEN each names the memory it actually has
    expect(mac.key).toBe("onboarding.ai_setup.recommend_measured_on_unified");
    expect(cpu.key).toBe("onboarding.ai_setup.recommend_measured_on");
  });
});
