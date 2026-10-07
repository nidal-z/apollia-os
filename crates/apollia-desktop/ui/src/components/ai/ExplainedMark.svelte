<script lang="ts">
  /**
   * ExplainedMark - an icon of the AI screen that marks a fact about a row
   * (the recommended variant, a model that runs whole on the accelerator),
   * with the text that explains it on hover. `label` and `explanation` are
   * required: a mark without them is not written.
   */
  import type { Snippet } from "svelte";
  import { Tooltip } from "$lib/components/ui/tooltip";
  import { cn } from "$lib/utils";

  interface Props {
    /** What the mark stands for, read by a screen reader. */
    label: string;
    /** What it means, shown on hover. */
    explanation: string;
    tone?: "primary" | "success";
    testid?: string;
    /** The icon. */
    children: Snippet;
  }

  let { label, explanation, tone = "primary", testid, children }: Props = $props();

  const TONES: Record<NonNullable<Props["tone"]>, string> = {
    primary: "border-primary/40 text-primary",
    success: "border-success/40 text-success-a11y",
  };
</script>

<Tooltip content={explanation} class="max-w-xs">
  <span
    class={cn("inline-flex h-5 w-5 items-center justify-center rounded border", TONES[tone])}
    role="img"
    aria-label={label}
    data-testid={testid}
  >
    {@render children()}
  </span>
</Tooltip>
