<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { GitBranch, Zap } from "lucide-svelte";
  import { slide } from "svelte/transition";
  import { Button } from "$lib/components/ui/button";

  interface PlanStep {
    step_id: string;
    description: string;
    tool_hint?: string;
    depends_on: string[];
  }

  interface TaskPlan {
    plan_id: string;
    task_id: string;
    steps: PlanStep[];
  }

  interface Props {
    /** Session identifier for correlating the choice with the alternatives. */
    sessionId: string;
    planA: TaskPlan;
    planB: TaskPlan;
    /** Temperature used to generate Plan A (shown in the card header). */
    tempA: number;
    /** Temperature used to generate Plan B (shown in the card header). */
    tempB: number;
    /** Called after the operator confirms a choice. */
    onChosen?: (chosen: "plan_a" | "plan_b") => void;
  }

  let {
    sessionId,
    planA,
    planB,
    tempA,
    tempB,
    onChosen,
  }: Props = $props();

  let isProcessing = $state(false);
  let error = $state<string | null>(null);
  let chosen = $state<"plan_a" | "plan_b" | null>(null);

  async function choosePlan(which: "plan_a" | "plan_b"): Promise<void> {
    isProcessing = true;
    error = null;
    try {
      await invoke("choose_plan", { sessionId, chosen: which });
      chosen = which;
      onChosen?.(which);
    } catch (err: unknown) {
      error = err instanceof Error ? err.message : String(err);
      isProcessing = false;
    }
  }
</script>

<div
  class="my-2 rounded-xl glass-surface glass-border-subtle border border-border p-4 space-y-4"
  transition:slide={{ duration: 200 }}
  data-testid="plan-alternatives-view"
>
  <!-- Header -->
  <div class="flex items-center gap-2 text-sm font-semibold text-foreground">
    <GitBranch class="h-4 w-4 text-primary" />
    <span>Choisissez un plan d'exécution</span>
  </div>

  <div class="grid grid-cols-1 gap-3 sm:grid-cols-2">
    <!-- Plan A card -->
    <div
      class="rounded-lg border p-3 space-y-2 transition-colors"
      class:border-primary={chosen === "plan_a"}
      class:border-border={chosen !== "plan_a"}
      data-testid="plan-a-card"
    >
      <div class="flex items-center justify-between">
        <span class="text-xs font-medium text-muted-foreground uppercase tracking-wide">
          Plan A — conservateur
        </span>
        <span class="text-[10px] text-muted-foreground bg-muted rounded px-1.5 py-0.5">
          t={tempA}
        </span>
      </div>

      <ol class="space-y-1 text-[11px] text-foreground list-decimal list-inside">
        {#each planA.steps as step (step.step_id)}
          <li class="leading-snug">{step.description}</li>
        {:else}
          <li class="text-muted-foreground italic">Aucune étape.</li>
        {/each}
      </ol>

      <Button
        variant={chosen === "plan_a" ? "default" : "outline"}
        size="sm"
        class="w-full h-7 text-[11px] mt-2"
        disabled={isProcessing || chosen !== null}
        onclick={() => choosePlan("plan_a")}
        data-testid="choose-plan-a"
      >
        {chosen === "plan_a" ? "✔ Sélectionné" : "Choisir Plan A"}
      </Button>
    </div>

    <!-- Plan B card -->
    <div
      class="rounded-lg border p-3 space-y-2 transition-colors"
      class:border-primary={chosen === "plan_b"}
      class:border-border={chosen !== "plan_b"}
      data-testid="plan-b-card"
    >
      <div class="flex items-center justify-between">
        <span class="text-xs font-medium text-muted-foreground uppercase tracking-wide">
          Plan B — exploratoire
        </span>
        <span class="text-[10px] text-muted-foreground bg-muted rounded px-1.5 py-0.5">
          t={tempB}
        </span>
      </div>

      <ol class="space-y-1 text-[11px] text-foreground list-decimal list-inside">
        {#each planB.steps as step (step.step_id)}
          <li class="leading-snug">{step.description}</li>
        {:else}
          <li class="text-muted-foreground italic">Aucune étape.</li>
        {/each}
      </ol>

      <Button
        variant={chosen === "plan_b" ? "default" : "outline"}
        size="sm"
        class="w-full h-7 text-[11px] mt-2"
        disabled={isProcessing || chosen !== null}
        onclick={() => choosePlan("plan_b")}
        data-testid="choose-plan-b"
      >
        {#if chosen === "plan_b"}
          <Zap class="h-3 w-3 mr-1" />
          Sélectionné
        {:else}
          Choisir Plan B
        {/if}
      </Button>
    </div>
  </div>

  {#if error}
    <p
      class="text-[11px] text-destructive"
      data-testid="plan-alternatives-error"
    >
      {error}
    </p>
  {/if}
</div>
