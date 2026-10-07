<script lang="ts">
  /**
   * ExplainedBadge - a badge of the AI screen, which carries the text that
   * explains it on hover. `explanation` is required: a badge without one is
   * not written.
   */
  import { CircleHelp } from "lucide-svelte";
  import { Tooltip } from "$lib/components/ui/tooltip";
  import { Badge } from "$lib/components/ui/badge";

  interface Props {
    /** What the badge reads. */
    label: string;
    /** What it means, shown on hover. */
    explanation: string;
    variant?: "neutral" | "primary" | "success" | "warning" | "info";
    /** Draws a question mark after the label, for a term the reader may not know. */
    help?: boolean;
    testid?: string;
  }

  let { label, explanation, variant = "neutral", help = false, testid }: Props = $props();
</script>

<Tooltip content={explanation}>
  <span class="inline-flex" data-testid={testid} aria-label="{label}. {explanation}">
    <Badge {variant} size="sm" outline>
      {label}
      {#if help}
        <CircleHelp size={10} strokeWidth={2} aria-hidden="true" />
      {/if}
    </Badge>
  </span>
</Tooltip>
