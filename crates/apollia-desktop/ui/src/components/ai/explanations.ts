/**
 * The icons and badges of the AI screen explain themselves on hover, by
 * construction: a capability names its label and its explanation here, and
 * `CapabilityIcon.svelte` reads both, so an icon without a text cannot be
 * written. `hoverExplanations.test.ts` holds the screen to it.
 */
import type { ModelCapabilities } from "$lib/types";

/** What a model can do, as the screen shows it. */
export type Capability = "vision" | "tools" | "reasoning";

/** Every capability, in display order. */
export const CAPABILITIES: readonly Capability[] = ["vision", "tools", "reasoning"];

const LABEL_KEYS: Record<Capability, string> = {
  vision: "ai.capability.vision.label",
  tools: "ai.capability.tools.label",
  reasoning: "ai.capability.reasoning.label",
};

const EXPLANATION_KEYS: Record<Capability, string> = {
  vision: "ai.capability.vision.explanation",
  tools: "ai.capability.tools.explanation",
  reasoning: "ai.capability.reasoning.explanation",
};

/** The catalogue key of the name of a capability. */
export function capabilityLabelKey(capability: Capability): string {
  return LABEL_KEYS[capability];
}

/** The catalogue key of the explanation shown on hover. */
export function capabilityExplanationKey(capability: Capability): string {
  return EXPLANATION_KEYS[capability];
}

/** The capabilities a model has, in display order; none when unknown. */
export function capabilitiesOf(caps: ModelCapabilities | null | undefined): Capability[] {
  if (!caps) return [];
  return CAPABILITIES.filter((c) => caps[c]);
}
