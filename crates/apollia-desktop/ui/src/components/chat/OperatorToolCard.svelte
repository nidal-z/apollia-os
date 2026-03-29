<script lang="ts">
  import type { ToolCallView } from "$lib/types";
  import { resolveToolDisplay } from "$lib/tools/tool-display";
  import { t } from "svelte-i18n";
  import { Loader2, Check, X } from "lucide-svelte";

  let { toolCall }: { toolCall: ToolCallView } = $props();

  const display = $derived(resolveToolDisplay(toolCall));
  const ToolIcon = $derived(display.icon);

  const isExecuted = $derived(toolCall.status === "executed");
  const isRefused = $derived(toolCall.status === "refused");
  const isPending = $derived(
    toolCall.status === "pending" || toolCall.status === "authorized",
  );

  const showOutputSummary = $derived(
    isExecuted && toolCall.output !== null && display.outputSummaryKey !== null,
  );

  /** Extracts and truncates the first paragraph of a refusal message. */
  const errorMessage = $derived.by((): string | null => {
    if (!isRefused || !toolCall.output) return null;
    const first = toolCall.output.split("\n")[0] ?? "";
    return first.length > 120 ? first.slice(0, 120) + "\u2026" : first;
  });
</script>

<div
  class="my-1 rounded-lg glass-surface glass-border-subtle px-3 py-2 text-xs
    {isRefused ? 'border-l-2 border-l-destructive' : ''}
    {isExecuted ? 'border-l-2 border-l-success' : ''}"
  data-testid="operator-tool-{toolCall.tool_name}"
>
  <!-- Header row: icon · description · status indicator -->
  <div class="flex items-center gap-2">
    <div
      class="flex h-6 w-6 flex-shrink-0 items-center justify-center rounded-md
        {isRefused ? 'bg-destructive/10' : isExecuted ? 'bg-success/10' : 'glass-inset'}"
    >
      <ToolIcon class="h-4 w-4 text-muted-foreground" />
    </div>

    <span class="min-w-0 flex-1 text-[13px] font-medium text-foreground">
      {$t(display.descriptionKey, { values: display.templateParams })}
    </span>

    <span class="ml-auto flex-shrink-0">
      {#if isPending}
        <Loader2 class="h-3 w-3 animate-spin text-muted-foreground" />
      {:else if isExecuted}
        <Check class="h-3 w-3 text-success" />
      {:else if isRefused}
        <X class="h-3 w-3 text-destructive" />
      {/if}
    </span>
  </div>

  <!-- Output summary (executed + output available) -->
  {#if showOutputSummary}
    <p class="mt-1 text-[11px] text-muted-foreground">
      {$t(display.outputSummaryKey!, { values: display.outputParams })}
    </p>
  {/if}

  <!-- Simplified error message (refused + output available) -->
  {#if isRefused && errorMessage}
    <p class="mt-1 text-[11px] text-destructive">{errorMessage}</p>
  {/if}
</div>
