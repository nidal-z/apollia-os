/**
 * Turning a ranked recommendation into something the step can render.
 *
 * The backend deliberately sends tags rather than sentences: the desktop is
 * translated, and a reason arriving as English prose would have to be parsed
 * back apart on this side to be shown in French. So every reason is a
 * discriminated union, and this module maps each case to an i18n key plus the
 * values that key interpolates.
 *
 * It is a plain module rather than logic inside the component for the reason
 * `aiSetupRules.ts` gives: these are pure decisions, and pure decisions are
 * worth testing without mounting anything.
 */
import type {
  HardwareProfileView,
  MemoryPool,
  RecommendCaveat,
  RecommendReason,
  RecommendedModel,
} from "$lib/ipc/models";

/** An i18n key with the values it interpolates. */
export interface Localisable {
  key: string;
  values?: Record<string, string | number>;
}

/** Bytes in a gibibyte, matching how the backend reports its estimates. */
const BYTES_PER_GIB = 1024 * 1024 * 1024;

/**
 * The label a model row carries, for example `"Qwen3 14B"`.
 *
 * Built from the family label and the parameter count rather than from the file
 * name: a file is called `Qwen_Qwen3-14B-Q4_K_M.gguf` when bartowski published
 * it, which names the quantiser more prominently than the model.
 */
export function modelDisplayName(model: RecommendedModel): string {
  const params = Number.isInteger(model.params_b)
    ? String(model.params_b)
    : model.params_b.toFixed(1);
  return `${model.family_label} ${params}B`;
}

/**
 * Whether a row should say how its layers are placed.
 *
 * Only worth showing when part of the model will run on the processor, which
 * is the case an operator can act on: a smaller model would run entirely on
 * the accelerator and considerably faster.
 */
export function needsOffloadWarning(model: RecommendedModel): boolean {
  return model.offload.n_gpu_layers > 0 && !model.offload.fully_offloaded;
}

/** A size for the row's secondary line, in the units the Hub reports. */
export function sizeLabel(bytes: number): string {
  const gb = bytes / BYTES_PER_GIB;
  if (gb >= 10) return `${Math.round(gb)} GB`;
  return `${gb.toFixed(1)} GB`;
}

/**
 * What the memory estimate should say under the model name.
 *
 * Names the cache separately from the weights, because the difference between
 * the two is the whole reason a 14B on a 16 GB machine is a tight fit rather
 * than a comfortable one, and an operator who sees only the download size has
 * no way to understand why.
 *
 * The engine's own buffers are named too, so the three parts add up to the
 * total the row shows. An unmeasured cache is labelled as an approximation
 * instead of being shown with the same authority as a measured one.
 */
export function memoryLabel(model: RecommendedModel): Localisable {
  const { estimate } = model;
  return {
    key: estimate.kv_cache_measured
      ? "onboarding.ai_setup.memory_breakdown"
      : "onboarding.ai_setup.memory_breakdown_approx",
    values: {
      total: estimate.total_gb.toFixed(1),
      weights: estimate.weights_gb.toFixed(1),
      cache: estimate.kv_cache_gb.toFixed(1),
      overhead: estimate.overhead_gb.toFixed(1),
    },
  };
}

/**
 * The line saying what the recommendations were measured on.
 *
 * Names each memory with its own figure: system memory, and the card's video
 * memory when there is one. A single "usable" figure read as the machine's
 * whole memory when it was only the card's.
 */
export function measuredLabel(hardware: HardwareProfileView): Localisable {
  const ram = hardware.total_ram_gb.toFixed(0);
  const { accelerator } = hardware;
  const vram = typeof accelerator.vram_gb === "number" ? accelerator.vram_gb : 0;
  if (accelerator.kind === "apple_silicon") {
    return {
      key: "onboarding.ai_setup.recommend_measured_on_unified",
      values: { chip: String(accelerator.chip ?? hardware.cpu_model), ram },
    };
  }
  if ((accelerator.kind === "cuda" || accelerator.kind === "generic") && vram > 0) {
    return {
      key: "onboarding.ai_setup.recommend_measured_on_gpu",
      values: {
        cpu: hardware.cpu_model,
        ram,
        gpu: String(accelerator.device_name ?? ""),
        vram: vram.toFixed(0),
      },
    };
  }
  return {
    key: "onboarding.ai_setup.recommend_measured_on",
    values: { cpu: hardware.cpu_model, ram },
  };
}

/** Map one caveat to the sentence that explains it. */
export function caveatLabel(caveat: RecommendCaveat): Localisable {
  switch (caveat.kind) {
    case "no_chat_template":
      return { key: "onboarding.ai_setup.caveat_no_chat_template" };
    case "unverified_architecture":
      return {
        key: "onboarding.ai_setup.caveat_unverified_architecture",
        values: { architecture: caveat.architecture },
      };
    case "header_truncated":
      return {
        key: "onboarding.ai_setup.caveat_header_truncated",
        values: { fields: caveat.missing.join(", ") },
      };
    case "no_pre_tokenizer":
      return { key: "onboarding.ai_setup.caveat_no_pre_tokenizer" };
  }
}

/**
 * One sentence per memory pool, spelled out rather than built from the pool's
 * name so every key stays greppable and the catalogue guard can see it read.
 */
const FITS_KEY: Record<MemoryPool, string> = {
  gpu: "onboarding.ai_setup.reason_fits_gpu",
  unified: "onboarding.ai_setup.reason_fits_unified",
  system: "onboarding.ai_setup.reason_fits_system",
};
const TIGHT_KEY: Record<MemoryPool, string> = {
  gpu: "onboarding.ai_setup.reason_tight_gpu",
  unified: "onboarding.ai_setup.reason_tight_unified",
  system: "onboarding.ai_setup.reason_tight_system",
};

/** Map one reason to the sentence that explains it. */
export function reasonLabel(reason: RecommendReason): Localisable {
  switch (reason.reason) {
    case "fits_comfortably":
      return {
        key: FITS_KEY[reason.pool],
        values: {
          needs: reason.needs_gb.toFixed(1),
          budget: reason.budget_gb.toFixed(1),
        },
      };
    case "tight":
      return {
        key: TIGHT_KEY[reason.pool],
        values: {
          needs: reason.needs_gb.toFixed(1),
          budget: reason.budget_gb.toFixed(1),
        },
      };
    case "split_across_memory":
      return {
        key: "onboarding.ai_setup.reason_split",
        values: {
          gpu: reason.gpu_gb.toFixed(1),
          vram: reason.vram_gb.toFixed(0),
          system: reason.system_gb.toFixed(1),
        },
      };
    case "native_tool_calling":
      return { key: "onboarding.ai_setup.reason_native_tools" };
    case "supersedes_generation":
      return {
        key: "onboarding.ai_setup.reason_supersedes",
        values: { replaces: reason.replaces },
      };
    case "trained_context":
      return {
        key: "onboarding.ai_setup.reason_trained_context",
        values: {
          tokens: Math.round(reason.tokens / 1024),
          asked: Math.round(reason.asked_tokens / 1024),
        },
      };
    case "reduced_context":
      return {
        key: "onboarding.ai_setup.reason_reduced_context",
        values: {
          tokens: Math.round(reason.tokens / 1024),
          defaultTokens: Math.round(reason.default_tokens / 1024),
        },
      };
    case "fully_accelerated":
      return {
        key: "onboarding.ai_setup.reason_fully_accelerated",
        values: { layers: reason.layers },
      };
    case "partial_offload":
      return {
        key: "onboarding.ai_setup.reason_partial_offload",
        values: { gpu: reason.gpu_layers, total: reason.total_layers },
      };
    case "sparse_mixture":
      return {
        key: "onboarding.ai_setup.reason_sparse_mixture",
        values: { percent: reason.active_percent },
      };
    case "quantisation":
      return {
        key: "onboarding.ai_setup.reason_quantisation",
        values: { format: reason.format, retained: reason.retained_percent },
      };
    case "caveat":
      return caveatLabel(reason.caveat);
  }
}

/**
 * The reasons worth putting on a collapsed row, best first.
 *
 * A row has space for one line, and the ranking already decided the order, so
 * this takes the most informative positive reason rather than listing all of
 * them. Caveats are excluded here and surfaced by [`rowCaveats`], which the
 * template renders differently: a caution styled like a selling point reads as
 * a selling point.
 */
export function primaryReason(model: RecommendedModel): Localisable | null {
  const positive = model.reasons.find((r) => r.reason !== "caveat");
  return positive ? reasonLabel(positive) : null;
}

/** Every caveat on a model, for the row's warning line. */
export function rowCaveats(model: RecommendedModel): Localisable[] {
  return model.reasons
    .filter((r): r is Extract<RecommendReason, { reason: "caveat" }> => r.reason === "caveat")
    .map((r) => caveatLabel(r.caveat));
}

/**
 * Whether a model can drive tool calls.
 *
 * Mirrors the backend's own rule rather than re-deriving it loosely: a model
 * without a chat template answers prose but cannot run an agent, which is most
 * of what this runtime is for.
 */
export function supportsToolCalling(model: RecommendedModel): boolean {
  if (model.verdict.status === "rejected") return false;
  if (model.verdict.status === "supported") return true;
  return !model.verdict.caveats.some((c) => c.kind === "no_chat_template");
}

/**
 * The model the step should preselect.
 *
 * The backend returns its list best first, so this is the head. It is a named
 * function anyway, because "the default is the first entry" is an assumption
 * worth stating once where a test can hold it, rather than an index literal
 * spread through the template.
 */
export function defaultChoice(models: RecommendedModel[]): RecommendedModel | null {
  return models[0] ?? null;
}
