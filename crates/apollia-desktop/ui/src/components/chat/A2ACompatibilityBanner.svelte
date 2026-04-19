<script lang="ts">
  /**
   * US-SP42-044 — Compatibility banner.
   *
   * Surfaces a semver mismatch between the director's `required_skill_version`
   * and the worker's `worker_advertised_version`. Offers a one-click action to
   * switch to a compatible alternative worker when one is available.
   */
  import { AlertTriangle, XOctagon, ArrowRight } from "lucide-svelte";
  import type { A2ACompatibilityWarning } from "$lib/types";

  interface Props {
    warning: A2ACompatibilityWarning;
    onUseAlternative?: (agentName: string) => void;
  }

  let { warning, onUseAlternative }: Props = $props();

  const isIncompatible = $derived(warning.severity === "incompatible");

  function handleUseAlternative(): void {
    if (warning.alternative_agent && onUseAlternative) {
      onUseAlternative(warning.alternative_agent);
    }
  }
</script>

<div
  role="alert"
  class="flex items-start gap-2 rounded-md border px-3 py-2 text-[11px] {isIncompatible
    ? 'border-destructive/40 bg-destructive/10 text-destructive'
    : 'border-warning/40 bg-warning/10 text-warning'}"
  data-testid="a2a-compat-banner"
  data-severity={warning.severity}
>
  {#if isIncompatible}
    <XOctagon size={14} class="mt-0.5 shrink-0" />
  {:else}
    <AlertTriangle size={14} class="mt-0.5 shrink-0" />
  {/if}
  <div class="flex-1 min-w-0">
    <p class="font-medium">
      {warning.skill_id} — {warning.agent_name}
    </p>
    <p class="opacity-90">
      {warning.message}
      <span class="opacity-70">
        (required: {warning.required_version}, advertised: {warning.advertised_version})
      </span>
    </p>
  </div>
  {#if warning.alternative_agent}
    <button
      type="button"
      onclick={handleUseAlternative}
      class="inline-flex items-center gap-1 rounded border border-current/40 px-2 py-0.5 text-[10px] font-medium hover:bg-current/10"
      data-testid="a2a-compat-use-alt"
    >
      Use {warning.alternative_agent}
      <ArrowRight size={10} />
    </button>
  {/if}
</div>
