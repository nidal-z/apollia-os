<script lang="ts">
  import { t } from "svelte-i18n";
  import { AlertTriangle } from "lucide-svelte";
  import type { ThinkingContradiction } from "$lib/types";

  interface Props {
    contradiction: ThinkingContradiction;
    /** Called when the operator clicks the badge — typically scrolls to the cited turn. */
    onJump?: (turnId: string) => void;
  }

  let { contradiction, onJump }: Props = $props();

  function handleClick() {
    onJump?.(contradiction.turn_id);
  }
</script>

<button
  type="button"
  class="contradiction-badge"
  onclick={handleClick}
  title={contradiction.excerpt}
  data-testid="contradiction-badge"
>
  <AlertTriangle size={12} />
  <span class="label">
    {$t("chat.thinking.contradiction", { default: "Contradiction" })}
  </span>
  <span class="turn">#{contradiction.turn_id.slice(0, 8)}</span>
</button>

<style>
  .contradiction-badge {
    display: inline-flex;
    align-items: center;
    gap: 0.375rem;
    padding: 0.125rem 0.5rem;
    border-radius: 9999px;
    border: 1px solid hsl(var(--destructive) / 0.35);
    background-color: hsl(var(--destructive) / 0.08);
    color: hsl(var(--destructive));
    font-size: 11px;
    font-weight: 500;
    line-height: 1.4;
    cursor: pointer;
    transition: background-color 0.15s ease;
  }

  .contradiction-badge:hover {
    background-color: hsl(var(--destructive) / 0.14);
  }

  .label {
    white-space: nowrap;
  }

  .turn {
    opacity: 0.65;
    font-variant-numeric: tabular-nums;
  }
</style>
