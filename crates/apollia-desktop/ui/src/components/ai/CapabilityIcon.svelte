<script lang="ts">
  /**
   * CapabilityIcon - one capability of a model, its explanation on hover.
   *
   * The text comes from the capability itself (`explanations.ts`), so the icon
   * cannot be drawn without it.
   */
  import { Eye, Lightbulb, Wrench } from "lucide-svelte";
  import { t } from "svelte-i18n";
  import { Tooltip } from "$lib/components/ui/tooltip";
  import {
    capabilityExplanationKey,
    capabilityLabelKey,
    type Capability,
  } from "./explanations";

  let { capability }: { capability: Capability } = $props();

  const ICONS = { vision: Eye, tools: Wrench, reasoning: Lightbulb } as const;
  const TONES: Record<Capability, string> = {
    vision: "border-warning/40 text-warning-a11y",
    tools: "border-info/40 text-info",
    reasoning: "border-success/40 text-success-a11y",
  };

  const Icon = $derived(ICONS[capability]);
</script>

<Tooltip content={$t(capabilityExplanationKey(capability))}>
  <span
    class="inline-flex h-5 w-5 items-center justify-center rounded border {TONES[capability]}"
    role="img"
    aria-label={$t(capabilityLabelKey(capability))}
    data-testid="ai-capability-{capability}"
  >
    <Icon size={12} strokeWidth={1.75} aria-hidden="true" />
  </span>
</Tooltip>
