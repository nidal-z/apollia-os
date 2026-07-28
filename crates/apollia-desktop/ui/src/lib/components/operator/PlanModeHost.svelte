<script lang="ts">
  // Global host for the plan-mode approval gate. Mounted once at the app shell:
  // it owns the runtime-event subscription (cleaned up on destroy) and renders
  // the right surface from the shared store, adapting to the active UI mode.
  // Operator: compact review. Builder: detailed review with diff and edit entry.
  import { uiMode } from "$lib/stores/mode";
  import { planModeState, startPlanModeListener } from "$lib/stores/planMode";
  import PlanReview from "./PlanReview.svelte";
  import PlanReviewBuilder from "./PlanReviewBuilder.svelte";
  import PlanEditFlow from "./PlanEditFlow.svelte";

  $effect(() => {
    let unlisten: (() => void) | null = null;
    let cancelled = false;
    void startPlanModeListener().then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  });

  const state = $derived($planModeState);
  const runId = $derived(state.runId);
  const visible = $derived(
    runId !== null &&
      (state.status === "pending_approval" || state.status === "editing"),
  );
  const isBuilder = $derived($uiMode === "builder");
</script>

{#if visible && runId}
  <div
    class="fixed bottom-4 left-1/2 w-[min(600px,92vw)] -translate-x-1/2"
    style="z-index: var(--z-overlay);"
    data-testid="plan-mode-host"
  >
    {#if state.status === "editing"}
      <PlanEditFlow {runId} />
    {:else if isBuilder}
      <PlanReviewBuilder {runId} canEdit={true} />
    {:else}
      <PlanReview {runId} />
    {/if}
  </div>
{/if}
