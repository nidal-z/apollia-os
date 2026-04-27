<script lang="ts">
  /**
   * Inline performance hint.
   *
   * Lightbulb icon + short sentence ("Durée attendue: 400ms" or a faster
   * alternative suggestion). Tooltip provides the full text when the visual
   * hint is truncated. Renders nothing when `hint` is empty.
   */
  import { Lightbulb } from "lucide-svelte";

  interface Props {
    hint: string | null | undefined;
  }

  let { hint }: Props = $props();
</script>

{#if hint}
  <span
    class="inline-flex items-center gap-1 rounded bg-muted/30 px-1.5 py-0.5 text-[10px] text-muted-foreground"
    title={hint}
    data-testid="tool-performance-hint"
  >
    <Lightbulb class="h-3 w-3 text-amber-500/80" aria-hidden="true" />
    <span class="line-clamp-1 max-w-[28ch]">{hint}</span>
  </span>
{/if}
