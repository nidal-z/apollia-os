<script lang="ts">
  import { t } from "svelte-i18n";
  import { BrainCircuit, ChevronDown, ChevronRight } from "lucide-svelte";
  import type {
    ThinkingContradiction,
    ThinkingQuality,
    ThinkingState,
    ThinkingSummary,
  } from "$lib/types";
  import ContradictionBadge from "./ContradictionBadge.svelte";

  interface Props {
    thinking: ThinkingState;
    /** Builder persona shows raw + summary side-by-side; operator sees summary only by default. */
    showRaw?: boolean;
    onJumpToTurn?: (turnId: string) => void;
  }

  let { thinking, showRaw = false, onJumpToTurn }: Props = $props();

  let expanded = $state(showRaw);

  const summary: ThinkingSummary | null = $derived(thinking.summary);
  const contradiction: ThinkingContradiction | null = $derived(
    thinking.summary?.contradiction_with_previous ?? null,
  );
  const quality: ThinkingQuality | null = $derived(thinking.summary?.quality ?? null);

  const durationMs: number | null = $derived(
    thinking.ended_ms ? thinking.ended_ms - thinking.started_ms : null,
  );
</script>

<div class="thinking-panel" data-testid="thinking-panel">
  <header class="header">
    <BrainCircuit size={14} class="icon" />
    <span class="title">{$t("chat.thinking.title", { default: "Thinking" })}</span>
    {#if durationMs !== null}
      <span class="meta">{(durationMs / 1000).toFixed(1)}s</span>
    {/if}
    {#if thinking.tokens > 0}
      <span class="meta">{thinking.tokens} tok</span>
    {/if}
    {#if quality}
      <span
        class="quality quality-{quality}"
        title={$t(`chat.thinking.quality.${quality}.tooltip`, {
          default:
            quality === "high"
              ? "Explicit reasoning, alternatives weighed"
              : quality === "medium"
                ? "Coherent but surface-level"
                : "Vague or self-contradicting",
        })}
      >
        {$t(`chat.thinking.quality.${quality}.label`, {
          default: quality,
        })}
      </span>
    {/if}
    {#if contradiction}
      <ContradictionBadge {contradiction} onJump={onJumpToTurn} />
    {/if}
  </header>

  {#if summary?.summary}
    <p class="summary">{summary.summary}</p>
  {/if}

  {#if thinking.raw_content}
    <button
      type="button"
      class="toggle"
      onclick={() => (expanded = !expanded)}
      aria-expanded={expanded}
    >
      {#if expanded}
        <ChevronDown size={12} />
      {:else}
        <ChevronRight size={12} />
      {/if}
      {$t("chat.thinking.raw_toggle", {
        default: expanded ? "Hide raw thinking" : "Show raw thinking",
      })}
    </button>
    {#if expanded}
      <pre class="raw" data-testid="thinking-raw">{thinking.raw_content}</pre>
    {/if}
  {/if}
</div>

<style>
  .thinking-panel {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    padding: 0.625rem 0.75rem;
    border-radius: 0.5rem;
    border: 1px solid hsl(var(--border));
    background-color: hsl(var(--muted) / 0.35);
    font-size: 12px;
  }

  .header {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.5rem;
  }

  .title {
    font-weight: 600;
    color: hsl(var(--foreground));
  }

  .meta {
    font-size: 11px;
    color: hsl(var(--muted-foreground));
    font-variant-numeric: tabular-nums;
  }

  .quality {
    padding: 0.0625rem 0.4rem;
    border-radius: 9999px;
    font-size: 10px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  .quality-low {
    background-color: hsl(var(--destructive) / 0.15);
    color: hsl(var(--destructive));
  }

  .quality-medium {
    background-color: hsl(45 93% 47% / 0.18);
    color: hsl(38 92% 30%);
  }

  .quality-high {
    background-color: hsl(142 71% 45% / 0.15);
    color: hsl(142 71% 28%);
  }

  .summary {
    margin: 0;
    color: hsl(var(--foreground));
    line-height: 1.5;
  }

  .toggle {
    display: inline-flex;
    align-items: center;
    gap: 0.25rem;
    background: none;
    border: none;
    padding: 0;
    color: hsl(var(--muted-foreground));
    cursor: pointer;
    font-size: 11px;
  }

  .toggle:hover {
    color: hsl(var(--foreground));
  }

  .raw {
    margin: 0;
    padding: 0.5rem 0.625rem;
    background-color: hsl(var(--background));
    border: 1px solid hsl(var(--border));
    border-radius: 0.375rem;
    font-family:
      ui-monospace, SFMono-Regular, Menlo, Consolas, "Liberation Mono",
      monospace;
    font-size: 11px;
    line-height: 1.45;
    white-space: pre-wrap;
    word-break: break-word;
    max-height: 280px;
    overflow-y: auto;
    color: hsl(var(--muted-foreground));
  }
</style>
