<!--
  The card a free chat shows while a worker agent runs the turn on its behalf:
  who was delegated to, how long it has been running, the steps as they land,
  and the guard message when one fires.
-->
<script lang="ts">
  import { t } from "svelte-i18n";
  import { Zap, Check, X } from "lucide-svelte";
  import { Spinner } from "$lib/components/ui/progress";
  import type { A2AStepView } from "./useA2ADelegation.svelte";

  interface Props {
    target: string;
    skillId: string;
    /** Elapsed seconds; zero hides the counter. */
    elapsed: number;
    steps: A2AStepView[];
    guardMessage: string | null;
  }

  let { target, skillId, elapsed, steps, guardMessage }: Props = $props();
</script>

<div class="flex justify-start" data-testid="chat-a2a-delegating">
  <div class="w-full overflow-hidden rounded-lg bg-surface-1 border border-border/60 border-l-2 border-l-secondary px-2.5 py-2">
    <div class="flex items-center gap-1.5">
      <Zap size={11} class="animate-pulse text-secondary" />
      <span class="text-caption font-medium text-secondary/80">
        {$t("chat.a2a_delegating", { values: { agent: target, skill: skillId } })}
      </span>
      {#if elapsed > 0}
        <span class="ml-auto flex-shrink-0 text-micro text-muted-foreground/40">{elapsed}s</span>
      {/if}
    </div>

    {#if steps.length > 0}
      <div class="mt-1.5 space-y-0.5">
        {#each steps as step (step.step_id)}
          <div class="flex items-center gap-1.5">
            <div class="flex-shrink-0">
              {#if step.status === "running"}
                <Spinner size={9} class="text-secondary/60" />
              {:else if step.status === "done"}
                <Check size={9} class="text-success/70" />
              {:else}
                <X size={9} class="text-destructive/70" />
              {/if}
            </div>
            <span class="truncate text-caption text-muted-foreground"
              >{step.desc ||
                $t("chat.a2a.step", { values: { n: step.step_num } })}</span
            >
            {#if step.total > 0}
              <span class="flex-shrink-0 text-micro text-muted-foreground/40">{step.step_num}/{step.total}</span>
            {/if}
            {#if step.durationMs !== undefined}
              <span class="ml-auto flex-shrink-0 text-micro text-muted-foreground/40">{step.durationMs}ms</span>
            {/if}
          </div>
        {/each}
      </div>
    {/if}

    {#if guardMessage}
      <div class="mt-1.5 rounded bg-destructive/10 px-2 py-1 text-micro text-destructive/80">
        {guardMessage}
      </div>
    {/if}
  </div>
</div>
