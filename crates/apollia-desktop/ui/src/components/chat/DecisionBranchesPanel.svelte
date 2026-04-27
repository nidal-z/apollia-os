<script lang="ts">
  /**
   * Decision branches panel.
   *
   * Renders a significant decision point (tool choice, agent delegate, memory
   * write/no-write) as a collapsible panel: the chosen option in green, and
   * up to 3 rejected alternatives in grey with their rejection reason and a
   * negative-going confidence bar.
   *
   * Opt-in: the backend only emits the underlying `DecisionPointRecorded`
   * event when `routines.decision_branches` is enabled — this component
   * simply assumes the caller already has a `DecisionPoint` value to render.
   */
  import { t } from "svelte-i18n";
  import { ChevronDown, ChevronRight, GitBranch, Check, X } from "lucide-svelte";
  import type { DecisionPoint, DecisionKind } from "$lib/types";

  interface Props {
    point: DecisionPoint;
    /** Builder sees the full panel expanded; operator sees a one-line summary. */
    skin?: "builder" | "operator";
  }

  let { point, skin = "builder" }: Props = $props();

  let expanded = $state(skin === "builder");

  function kindLabel(kind: DecisionKind): string {
    switch (kind) {
      case "tool_choice":
        return "Tool choice";
      case "agent_delegate":
        return "Agent delegate";
      case "memory_write":
        return "Memory write";
      default:
        return "Decision";
    }
  }

  function clampDelta(delta: number): number {
    // Alternatives should always be ≤ 0; clamp to [-1, 0] for safe rendering.
    if (Number.isNaN(delta)) return 0;
    return Math.max(-1, Math.min(0, delta));
  }
</script>

<section
  class="decision-branches"
  data-testid="decision-branches-panel"
  data-kind={point.kind}
>
  <button
    type="button"
    class="header"
    onclick={() => (expanded = !expanded)}
    aria-expanded={expanded}
  >
    {#if expanded}
      <ChevronDown size={12} />
    {:else}
      <ChevronRight size={12} />
    {/if}
    <GitBranch size={12} class="icon" />
    <span class="title">
      {$t("chat.decision_branches.title", {
        default: "Decision branches",
      })}
    </span>
    <span class="kind">{kindLabel(point.kind)}</span>
    <span class="chosen-inline" title={point.chosen}>
      <Check size={11} class="check" />
      {point.chosen}
    </span>
    {#if point.alternatives.length > 0}
      <span class="count">+{point.alternatives.length} alt</span>
    {/if}
  </button>

  {#if expanded}
    <ol class="list">
      <li class="row chosen">
        <span class="marker">
          <Check size={12} />
        </span>
        <span class="label">{point.chosen}</span>
        <span class="reason">
          {$t("chat.decision_branches.chosen", { default: "chosen" })}
        </span>
      </li>
      {#each point.alternatives as alt, i (`${alt.label}-${i}`)}
        <li class="row alt">
          <span class="marker">
            <X size={12} />
          </span>
          <span class="label">{alt.label}</span>
          <span class="reason">{alt.rejected_reason}</span>
          <span
            class="delta"
            title={`confidence Δ ${alt.confidence_delta.toFixed(2)}`}
          >
            <span
              class="bar"
              style={`width: ${Math.round(Math.abs(clampDelta(alt.confidence_delta)) * 100)}%`}
            ></span>
          </span>
        </li>
      {/each}
      {#if point.alternatives.length === 0}
        <li class="row empty">
          <span class="reason">
            {$t("chat.decision_branches.no_alternatives", {
              default: "No alternatives were weighed.",
            })}
          </span>
        </li>
      {/if}
    </ol>
  {/if}
</section>

<style>
  .decision-branches {
    display: flex;
    flex-direction: column;
    gap: 0.375rem;
    padding: 0.5rem 0.625rem;
    border-radius: 0.5rem;
    border: 1px solid hsl(var(--border));
    background-color: hsl(var(--muted) / 0.35);
    font-size: 12px;
  }

  .header {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 0.375rem;
    background: none;
    border: none;
    padding: 0;
    cursor: pointer;
    color: hsl(var(--foreground));
    text-align: left;
  }

  .title {
    font-weight: 600;
  }

  .kind {
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: hsl(var(--muted-foreground));
  }

  .chosen-inline {
    display: inline-flex;
    align-items: center;
    gap: 0.2rem;
    padding: 0.0625rem 0.4rem;
    border-radius: 9999px;
    font-size: 11px;
    background-color: hsl(142 71% 45% / 0.15);
    color: hsl(142 71% 28%);
    max-width: 18ch;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .chosen-inline :global(.check) {
    color: hsl(142 71% 35%);
  }

  .count {
    margin-left: auto;
    font-size: 10px;
    color: hsl(var(--muted-foreground));
  }

  .list {
    list-style: none;
    padding: 0;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }

  .row {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr) minmax(0, 2fr) auto;
    align-items: center;
    gap: 0.5rem;
    padding: 0.25rem 0.375rem;
    border-radius: 0.375rem;
  }

  .row.chosen {
    background-color: hsl(142 71% 45% / 0.12);
    color: hsl(142 71% 22%);
  }

  .row.alt {
    background-color: hsl(var(--muted) / 0.4);
    color: hsl(var(--muted-foreground));
  }

  .row.empty {
    grid-template-columns: 1fr;
    font-style: italic;
    color: hsl(var(--muted-foreground));
  }

  .marker {
    display: inline-flex;
    align-items: center;
    justify-content: center;
  }

  .label {
    font-weight: 600;
    color: hsl(var(--foreground));
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .row.alt .label {
    color: hsl(var(--muted-foreground));
  }

  .reason {
    font-size: 11px;
    color: hsl(var(--muted-foreground));
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .delta {
    width: 56px;
    height: 6px;
    border-radius: 9999px;
    background-color: hsl(var(--border));
    overflow: hidden;
    direction: rtl;
  }

  .bar {
    display: block;
    height: 100%;
    background-color: hsl(0 84% 60% / 0.6);
  }
</style>
