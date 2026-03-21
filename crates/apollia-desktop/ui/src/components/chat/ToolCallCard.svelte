<script lang="ts">
  import { t } from "svelte-i18n";
  import type { ToolCallView } from "$lib/types";
  import { slide } from "svelte/transition";
  import { Wrench, Check, X, Loader2, ChevronDown, ChevronUp } from "lucide-svelte";

  interface Props {
    toolCall: ToolCallView;
  }

  let { toolCall }: Props = $props();

  let showOutput = $state(false);

  const isExecuted = $derived(toolCall.status === "executed");
  const isRefused = $derived(toolCall.status === "refused");
  const isPending = $derived(toolCall.status === "pending" || toolCall.status === "authorized");

  const inputPreview = $derived.by(() => {
    const raw = JSON.stringify(toolCall.input);
    if (raw.length <= 100) return raw;
    return raw.slice(0, 100) + "\u2026";
  });

  const inputFull = $derived(JSON.stringify(toolCall.input, null, 2));
  const isInputTruncated = $derived(JSON.stringify(toolCall.input).length > 100);

  let showFullInput = $state(false);
</script>

<div
  class="my-1.5 rounded-xl border px-3 py-2 text-xs backdrop-blur-sm {isRefused
    ? 'border-[hsl(var(--destructive))]/30 bg-[hsl(var(--destructive))]/5'
    : isExecuted
      ? 'border-[hsl(var(--success))]/30 bg-[hsl(var(--success))]/5'
      : 'glass-border glass-inset'}"
  data-testid="tool-call-card-{toolCall.tool_name}"
  transition:slide={{ duration: 200 }}
>
  <!-- Header -->
  <div class="flex items-center gap-1.5">
    <div class="flex h-5 w-5 items-center justify-center rounded-md {isRefused
      ? 'bg-[hsl(var(--destructive))]/10'
      : isExecuted
        ? 'bg-[hsl(var(--success))]/10'
        : 'glass-inset'}">
      <Wrench class="h-2.5 w-2.5 text-muted-foreground" />
    </div>
    <span class="font-medium text-foreground">{toolCall.tool_name}</span>
    <span class="ml-auto">
      {#if isPending}
        <Loader2 class="h-3 w-3 animate-spin text-muted-foreground" />
      {:else if isExecuted}
        <Check class="h-3 w-3 text-[hsl(var(--success))]" />
      {:else if isRefused}
        <X class="h-3 w-3 text-[hsl(var(--destructive))]" />
      {/if}
    </span>
  </div>

  <!-- Input preview -->
  <div class="mt-1.5 text-muted-foreground">
    {#if showFullInput}
      <pre class="whitespace-pre-wrap break-all rounded-lg glass-inset p-2 font-mono text-[10px]">{inputFull}</pre>
    {:else}
      <span class="font-mono text-[10px]">{inputPreview}</span>
    {/if}
    {#if isInputTruncated}
      <button
        class="ml-1 text-[10px] text-primary hover:underline"
        onclick={() => (showFullInput = !showFullInput)}
      >
        {showFullInput ? $t("chat.tool_collapse") : $t("chat.tool_expand")}
      </button>
    {/if}
  </div>

  <!-- Output (collapsible) -->
  {#if toolCall.output}
    <div class="mt-1.5">
      <button
        class="flex items-center gap-1 text-[10px] text-primary hover:underline"
        onclick={() => (showOutput = !showOutput)}
        data-testid="tool-output-toggle-{toolCall.tool_name}"
      >
        {#if showOutput}
          <ChevronUp class="h-2.5 w-2.5" />
          <span>{$t("chat.tool_hide_result")}</span>
        {:else}
          <ChevronDown class="h-2.5 w-2.5" />
          <span>{$t("chat.tool_show_result")}</span>
        {/if}
      </button>
      {#if showOutput}
        <pre
          class="mt-1 max-h-48 overflow-auto whitespace-pre-wrap break-all rounded-lg glass-inset p-2 font-mono text-[10px] text-foreground"
          transition:slide={{ duration: 150 }}
        >{toolCall.output}</pre>
      {/if}
    </div>
  {/if}
</div>
