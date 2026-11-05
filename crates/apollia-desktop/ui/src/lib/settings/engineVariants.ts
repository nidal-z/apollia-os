// The variants of the inference engine the followed channel offers for this
// platform, each with the verdict of the runtime on this machine.
//
// Nothing here rates a variant: the verdict and its reason come from the
// engine list of the runtime (`ratings`), the packages from `available`. This
// module joins the two and says which sentence of the catalogue words each
// reason.

import type {
  EngineAvailablePackage,
  EngineEntry,
  EngineRating,
  EngineRatingReason,
  EngineVariant,
  EngineVerdict,
  EnginesResponse,
} from "$lib/ipc/system";

/** The engine whose variants the runtime rates. */
const RATED_ENGINE: EngineAvailablePackage["engine"] = "llama-server";

/** One variant of the channel, as the list of the page shows it. */
export interface VariantRow {
  variant: EngineVariant;
  /** The newest build the channel offers for it. */
  tag: string;
  /** Archive size of that build, in bytes. */
  size: number;
  /** True when a build of this variant offered by the channel is installed here. */
  installed: boolean;
  /** The verdict of the runtime, `null` when the hardware is not known. */
  verdict: EngineVerdict | null;
  reason: EngineRatingReason | null;
  /** True for ROCm under Windows, which the channel publishes as experimental. */
  experimental: boolean;
}

/** What the variant list has to show. */
export type VariantListState =
  /** No verified manifest is cached yet: nothing is known of the channel. */
  | { kind: "unread" }
  /** The channel offers no variant for this platform: the shipped engine serves. */
  | { kind: "none" }
  | { kind: "rows"; rows: VariantRow[] };

const VERDICT_ORDER: Record<EngineVerdict, number> = {
  recommended: 0,
  compatible: 1,
  incompatible: 2,
};

/** Whether `platform` names Windows, as `get_system_info` states the system (`windows x86_64`). */
export function isWindows(os: string | null | undefined): boolean {
  return (os ?? "").toLowerCase().startsWith("windows");
}

/**
 * The variants of the channel for this platform, installed or not, each with
 * its rating; recommended first, incompatible last, the order of the runtime
 * kept within a verdict.
 */
export function variantList(
  response: EnginesResponse | null,
  os: string | null = null,
): VariantListState {
  if (!response) return { kind: "unread" };
  const available = response.available ?? [];
  const ratings = response.ratings ?? [];
  if (response.manifest_sequence === null && available.length === 0 && ratings.length === 0) {
    return { kind: "unread" };
  }
  const byVariant = new Map<EngineVariant, VariantRow>();
  for (const pkg of available) {
    if (pkg.engine !== RATED_ENGINE) continue;
    const kept = byVariant.get(pkg.variant);
    const rating: EngineRating | undefined = ratings.find((r) => r.variant === pkg.variant);
    byVariant.set(pkg.variant, {
      variant: pkg.variant,
      tag: pkg.tag,
      size: pkg.size,
      installed: (kept?.installed ?? false) || pkg.installed,
      verdict: rating?.verdict ?? null,
      reason: rating?.reason ?? null,
      experimental:
        pkg.variant === "rocm" && isWindows(os) && rating?.verdict === "compatible" && !rating.reason,
    });
  }
  const order = (variant: EngineVariant): number => {
    const at = ratings.findIndex((r) => r.variant === variant);
    return at === -1 ? ratings.length : at;
  };
  const rows = [...byVariant.values()].sort((a, b) => {
    const verdict =
      VERDICT_ORDER[a.verdict ?? "compatible"] - VERDICT_ORDER[b.verdict ?? "compatible"];
    return verdict !== 0 ? verdict : order(a.variant) - order(b.variant);
  });
  return rows.length === 0 ? { kind: "none" } : { kind: "rows", rows };
}

/** The rows shown with the incompatible ones folded or not. */
export function shownVariants(rows: VariantRow[], showIncompatible: boolean): VariantRow[] {
  return showIncompatible ? rows : rows.filter((row) => row.verdict !== "incompatible");
}

/** How many rows the fold hides. */
export function incompatibleCount(rows: VariantRow[]): number {
  return rows.filter((row) => row.verdict === "incompatible").length;
}

/** Display name of a variant: a backend name, or the processor. */
export function variantLabel(variant: EngineEntry["variant"], cpuLabel: string): string | null {
  switch (variant) {
    case "metal":
      return "Metal";
    case "cuda-12":
      return "CUDA 12";
    case "cuda-13":
      return "CUDA 13";
    case "rocm":
      return "ROCm";
    case "vulkan":
      return "Vulkan";
    case "cpu":
      return cpuLabel;
    case null:
      return null;
  }
}

/** The verdict of `variant`, or `null` when the runtime did not rate it. */
export function verdictOf(
  response: EnginesResponse | null,
  variant: EngineVariant | null,
): EngineVerdict | null {
  if (!response || variant === null) return null;
  return (response.ratings ?? []).find((r) => r.variant === variant)?.verdict ?? null;
}

/** The i18n key of each verdict's badge. */
export const VERDICT_KEYS: Record<EngineVerdict, string> = {
  recommended: "settings.engines.verdict_recommended",
  compatible: "settings.engines.verdict_compatible",
  incompatible: "settings.engines.verdict_incompatible",
};

/** A catalogue sentence and the values it is filled with. */
export interface Sentence {
  key: string;
  values: Record<string, string>;
}

/** The i18n key of each reason code. */
export const REASON_KEYS: Record<EngineRatingReason["code"], string> = {
  driver_too_old: "settings.engines.reason.driver_too_old",
  arch_dropped_in_cuda13: "settings.engines.reason.arch_dropped_in_cuda13",
  gfx_not_in_build: "settings.engines.reason.gfx_not_in_build",
  no_vulkan_loader: "settings.engines.reason.no_vulkan_loader",
  no_nvidia_driver: "settings.engines.reason.no_nvidia_driver",
  not_built_for_platform: "settings.engines.reason.not_built_for_platform",
  no_gpu: "settings.engines.reason.no_gpu",
  engine_sees_no_device: "settings.engines.reason.engine_sees_no_device",
  no_rocm_runtime: "settings.engines.reason.no_rocm_runtime",
};

/** On a recommended variant an old driver is a caveat, worded as what to do about it. */
const DRIVER_TOO_OLD_ACTION_KEY = "settings.engines.reason.driver_too_old_action";

/**
 * The sentence that words `reason`. On a recommended variant the reason is a
 * caveat, and an old driver reads as what to do about it.
 */
export function reasonSentence(reason: EngineRatingReason, verdict: EngineVerdict | null): Sentence {
  switch (reason.code) {
    case "driver_too_old":
      return {
        key: verdict === "recommended" ? DRIVER_TOO_OLD_ACTION_KEY : REASON_KEYS.driver_too_old,
        values: { have: reason.have, need: reason.need },
      };
    case "arch_dropped_in_cuda13":
      return { key: REASON_KEYS.arch_dropped_in_cuda13, values: { cc: reason.cc } };
    case "gfx_not_in_build":
      return { key: REASON_KEYS.gfx_not_in_build, values: { gfx: reason.gfx } };
    default:
      return { key: REASON_KEYS[reason.code], values: {} };
  }
}
