<script lang="ts">
  /**
   * Step-level drill-down for A2A invocations.
   *
   * Renders each [`A2AStepProvenance`] entry scrollable to the matching item in
   * TimelineGlobal. The `step_id` is the shared correlation key.
   */
  import { ArrowDownRight, Search } from "lucide-svelte";
  import type { A2AStepProvenance } from "$lib/types";

  interface Props {
    steps: A2AStepProvenance[];
    onStepClick?: (stepId: string) => void;
  }

  let { steps, onStepClick }: Props = $props();

  function formatTime(ts: number): string {
    try {
      return new Date(ts).toLocaleTimeString(undefined, {
        hour: "2-digit",
        minute: "2-digit",
        second: "2-digit",
      });
    } catch {
      return "";
    }
  }

  function handleClick(stepId: string): void {
    if (onStepClick) onStepClick(stepId);
    window.dispatchEvent(
      new CustomEvent("timeline:scroll-to-step", { detail: { stepId } }),
    );
  }
</script>

<div class="flex flex-col gap-1.5" data-testid="a2a-step-tree">
  {#if steps.length === 0}
    <p class="px-2 py-3 text-[11px] text-muted-foreground/70">No steps recorded yet.</p>
  {:else}
    {#each steps as step (step.step_id)}
      <button
        type="button"
        onclick={() => handleClick(step.step_id)}
        class="flex w-full flex-col gap-1 rounded-md border border-border/40 bg-muted/30 px-2 py-1.5 text-left text-[11px] hover:bg-muted/60 transition-colors"
        data-testid="a2a-step-row-{step.step_id}"
      >
        <div class="flex items-center gap-1.5">
          {#if step.parent_step}
            <ArrowDownRight size={11} class="text-muted-foreground/60" />
          {/if}
          <span class="font-medium text-foreground">{step.agent_from}</span>
          <span class="text-muted-foreground/60">→</span>
          <span class="font-medium text-foreground">{step.agent_to}</span>
          <span class="ml-1 rounded bg-secondary/20 px-1 text-[10px] text-secondary">
            {step.skill_id}
          </span>
          <span class="ml-auto text-[10px] text-muted-foreground/60">
            {formatTime(step.timestamp_ms)}
          </span>
          <Search size={10} class="opacity-50" />
        </div>
        <p class="text-muted-foreground truncate">
          <span class="text-muted-foreground/60">in:</span> {step.input_excerpt}
        </p>
        {#if step.output_excerpt}
          <p class="text-muted-foreground truncate">
            <span class="text-muted-foreground/60">out:</span> {step.output_excerpt}
          </p>
        {/if}
      </button>
    {/each}
  {/if}
</div>
