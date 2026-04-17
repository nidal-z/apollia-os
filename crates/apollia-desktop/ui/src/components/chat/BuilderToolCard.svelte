<script lang="ts">
  import type { ToolCallView } from "$lib/types";
  import {
    resolveToolDisplay,
    resolveOutputParams,
    buildOutputSummary,
    buildBashInputDisplay,
    buildHttpInputDisplay,
  } from "$lib/tools/tool-display";
  import { t } from "svelte-i18n";
  import { Loader2, Check, X } from "lucide-svelte";
  import WebSearchResultsCard from "./WebSearchResultsCard.svelte";
  import WebReadCard from "./WebReadCard.svelte";

  let { toolCall }: { toolCall: ToolCallView } = $props();

  const display = $derived(resolveToolDisplay(toolCall));
  const ToolIcon = $derived(display.icon);
  const outputParams = $derived(resolveOutputParams(toolCall));

  let showFullInput = $state(false);
  let showOutput = $state(false);

  const isExecuted = $derived(toolCall.status === "executed");
  const isRefused = $derived(toolCall.status === "refused");
  const isPending = $derived(
    toolCall.status === "pending" || toolCall.status === "authorized",
  );

  const inputJson = $derived(JSON.stringify(toolCall.input, null, 2));
  const hasLongInput = $derived(inputJson.length > 200);
  const inputPreview = $derived(
    hasLongInput ? inputJson.slice(0, 200) + "..." : inputJson,
  );

  const bashDisplay = $derived(
    toolCall.tool_name === "bash_executor"
      ? buildBashInputDisplay(toolCall.input)
      : null,
  );
  const httpDisplay = $derived(
    toolCall.tool_name === "http_fetch"
      ? buildHttpInputDisplay(toolCall.input)
      : null,
  );

  const outputSummary = $derived(buildOutputSummary(toolCall.tool_name, outputParams));
  const outputToggleLabel = $derived(
    outputSummary !== null
      ? `${$t("chat.tool_show_result")} (${outputSummary})`
      : $t("chat.tool_show_result"),
  );
</script>

{#if toolCall.tool_name === "web_search"}
  <WebSearchResultsCard {toolCall} />
{:else if toolCall.tool_name === "web_read"}
  <WebReadCard {toolCall} />
{:else}
<div
  class="my-1 rounded-lg glass-surface glass-border-subtle px-3 py-2 text-xs
    {isRefused ? 'border-l-2 border-l-destructive' : ''}
    {isExecuted ? 'border-l-2 border-l-success' : ''}"
  data-testid="builder-tool-{toolCall.tool_name}"
>
  <!-- Header: icon · tool_name · status -->
  <div class="flex items-center gap-1.5">
    <div
      class="flex h-5 w-5 flex-shrink-0 items-center justify-center rounded-md
        {isRefused ? 'bg-destructive/10' : isExecuted ? 'bg-success/10' : 'glass-inset'}"
    >
      <ToolIcon class="h-3 w-3 text-muted-foreground" />
    </div>

    <span class="font-medium font-mono text-[12px] text-foreground min-w-0 flex-1 truncate">
      {toolCall.tool_name}
    </span>

    <span class="ml-auto flex flex-shrink-0 items-center gap-1">
      {#if isPending}
        <Loader2 class="h-3 w-3 animate-spin text-muted-foreground" />
      {:else if isExecuted}
        <Check class="h-3 w-3 text-success" />
        {#if toolCall.duration_ms != null}
          <span class="text-[10px] text-muted-foreground">{toolCall.duration_ms}ms</span>
        {/if}
      {:else if isRefused}
        <X class="h-3 w-3 text-destructive" />
        {#if toolCall.exit_code != null}
          <span class="text-[10px] text-destructive">exit {toolCall.exit_code}</span>
        {/if}
      {/if}
    </span>
  </div>

  <!-- Input display -->
  <div class="mt-1.5">
    {#if bashDisplay !== null}
      <pre
        class="rounded bg-muted/40 px-2 py-1 text-[11px] font-mono text-foreground overflow-x-auto"
      ><code>{bashDisplay}</code></pre>
    {:else if httpDisplay !== null}
      <p class="text-[11px] font-mono text-muted-foreground">{httpDisplay}</p>
    {:else}
      <pre
        class="rounded bg-muted/40 px-2 py-1 text-[11px] font-mono text-foreground overflow-x-auto whitespace-pre-wrap break-all"
      ><code>{showFullInput ? inputJson : inputPreview}</code></pre>
      {#if hasLongInput}
        <button
          class="mt-0.5 text-[10px] text-muted-foreground hover:text-foreground transition-colors"
          onclick={() => (showFullInput = !showFullInput)}
        >
          {showFullInput ? "show less" : "show all"}
        </button>
      {/if}
    {/if}
  </div>

  <!-- Collapsible output -->
  {#if isExecuted && toolCall.output !== null}
    <div class="mt-1.5">
      <button
        class="text-[10px] text-muted-foreground hover:text-foreground transition-colors"
        onclick={() => (showOutput = !showOutput)}
      >
        {showOutput ? $t("chat.tool_hide_result") : outputToggleLabel}
      </button>
      {#if showOutput}
        <pre
          class="mt-1 rounded bg-muted/40 px-2 py-1 text-[11px] font-mono text-foreground overflow-x-auto whitespace-pre-wrap break-all"
        ><code>{toolCall.output}</code></pre>
      {/if}
    </div>
  {/if}
</div>
{/if}
