<script lang="ts">
  // Considered-alternatives popover for a plan step's decision point.
  //
  // Surfaces the options the agent weighed but did not take, each with its
  // label, rejection reason and confidence delta. The decision dimension is
  // Builder-only: in Operator the ghost list is empty and nothing renders. A
  // turn with no decision point (or no rejected alternative) renders nothing,
  // so there is never an empty trigger or an empty popover. Built on the
  // canonical `Popover` primitive (bits-ui under the hood); no invoke here.
  import { t } from "svelte-i18n";
  import { uiMode } from "$lib/stores/mode";
  import { Popover } from "$lib/components/ui/popover";
  import { toGhostNodes } from "./decisionGhostNodes";
  import type { DecisionPoint } from "$lib/types";
  import { PLAN_SESSION_KEYS } from "$lib/i18n/strings/planSession";

  type Props = {
    point: DecisionPoint | null;
  };
  let { point }: Props = $props();

  const ghosts = $derived($uiMode === "builder" ? toGhostNodes(point) : []);
  const showPopover = $derived(ghosts.length > 0);
  const chosen = $derived(point?.chosen ?? "");
</script>

{#if showPopover}
  <div data-testid="plan-decision-popover">
    <Popover side="bottom" align="start">
      {#snippet trigger(props)}
        <button
          {...props}
          type="button"
          class="text-[11px] text-muted-foreground underline transition-colors hover:text-foreground"
        >
          {$t(PLAN_SESSION_KEYS.alternativesTrigger, {
            values: { count: ghosts.length },
          })}
        </button>
      {/snippet}
      {#snippet content()}
        {#if chosen}
          <p class="mb-1 text-[11px] text-success">
            <span class="font-medium">{$t(PLAN_SESSION_KEYS.chosenLabel)}:</span>
            {chosen}
          </p>
        {/if}
        <ul class="space-y-2">
          {#each ghosts as ghost (ghost.id)}
            <li class="rounded border px-2 py-1 text-[11px] {ghost.tokenClass}">
              <p class="font-medium">{ghost.label}</p>
              <p>
                <span class="font-medium"
                  >{$t(PLAN_SESSION_KEYS.rejectedReasonLabel)}:</span
                >
                {ghost.rejectedReason}
              </p>
              <p>
                <span class="font-medium"
                  >{$t(PLAN_SESSION_KEYS.confidenceDeltaLabel)}:</span
                >
                <span class={ghost.confidenceDelta < 0 ? "text-destructive" : "text-success"}>
                  {ghost.confidenceDelta}
                </span>
              </p>
            </li>
          {/each}
        </ul>
      {/snippet}
    </Popover>
  </div>
{/if}
