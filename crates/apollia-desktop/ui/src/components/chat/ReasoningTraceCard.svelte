<script lang="ts">
  import type { ToolCallView } from "$lib/types";
  import {
    BrainCircuit,
    ChevronDown,
    ChevronRight,
    Check,
    X,
    Loader2,
  } from "lucide-svelte";
  import { slide } from "svelte/transition";
  import { t } from "svelte-i18n";
  import BuilderToolCard from "./BuilderToolCard.svelte";
  import OperatorToolCard from "./OperatorToolCard.svelte";
  import ApprovalCard from "./ApprovalCard.svelte";
  import OperatorApprovalCard from "./OperatorApprovalCard.svelte";

  interface Props {
    toolCalls: ToolCallView[];
    /** Optional thinking text stored in message.metadata.thinking_trace */
    thinkingText?: string | null;
    sessionId: string;
    messageId: string;
    isOperator: boolean;
  }

  let {
    toolCalls,
    thinkingText = null,
    sessionId,
    messageId,
    isOperator,
  }: Props = $props();

  const hasPending = $derived(
    toolCalls.some((c) => c.status === "pending" || c.status === "authorized"),
  );

  // null = automatic (expand when pending / approval needed / thinking available)
  // boolean = explicit user toggle
  let userExpanded = $state<boolean | null>(null);
  const isExpanded = $derived(
    userExpanded !== null ? userExpanded : (hasPending || !!thinkingText),
  );

  // When a pending approval appears mid-stream, reopen automatically
  $effect(() => {
    if (hasPending) userExpanded = null;
  });

  function toggle() {
    userExpanded = !isExpanded;
  }

  const executedCount = $derived(
    toolCalls.filter((c) => c.status === "executed").length,
  );
  const refusedCount = $derived(
    toolCalls.filter((c) => c.status === "refused").length,
  );
  const pendingCount = $derived(
    toolCalls.filter((c) => c.status === "pending" || c.status === "authorized")
      .length,
  );
  const total = $derived(toolCalls.length);

  // Collapsed chain preview: up to 3 names with →, then "+N"
  const chainPreview = $derived(
    toolCalls
      .slice(0, 3)
      .map((c) => c.tool_name)
      .join(" → ") + (total > 3 ? ` +${total - 3}` : ""),
  );
</script>

<div
  class="mt-2 overflow-hidden rounded-lg border border-border/20 glass-surface"
  data-testid="reasoning-trace-card"
>
  <!-- Collapsed header — always visible -->
  <button
    onclick={toggle}
    class="flex w-full items-center gap-2 px-2.5 py-1.5 text-left transition-colors hover:bg-muted/25"
    aria-expanded={isExpanded}
  >
    <!-- Status icon -->
    <div class="flex-shrink-0">
      {#if pendingCount > 0}
        <Loader2 size={11} class="animate-spin text-primary/60" />
      {:else}
        <BrainCircuit size={11} class="text-muted-foreground/50" />
      {/if}
    </div>

    <!-- Chain preview + optional thinking hint -->
    <div class="min-w-0 flex-1 overflow-hidden">
      <span class="block truncate font-mono text-[11px] text-muted-foreground/70">
        {chainPreview}
      </span>
      {#if thinkingText && !isExpanded}
        <span class="block truncate text-[10px] italic text-muted-foreground/50">
          {thinkingText.slice(0, 120)}{thinkingText.length > 120 ? "..." : ""}
        </span>
      {/if}
    </div>

    <!-- Status badges -->
    <div class="flex flex-shrink-0 items-center gap-2">
      {#if executedCount > 0}
        <span class="flex items-center gap-0.5 text-[10px] text-success/80">
          <Check size={9} />
          {#if total > 1}<span>{executedCount}</span>{/if}
        </span>
      {/if}
      {#if refusedCount > 0}
        <span class="flex items-center gap-0.5 text-[10px] text-destructive/80">
          <X size={9} />
          <span>{refusedCount}</span>
        </span>
      {/if}
      {#if pendingCount > 0}
        <span class="text-[10px] text-primary/70">
          {$t("chat.reasoning_pending", { values: { n: pendingCount } })}
        </span>
      {/if}
    </div>

    <!-- Chevron -->
    <div class="flex-shrink-0 text-muted-foreground/40">
      {#if isExpanded}
        <ChevronDown size={11} />
      {:else}
        <ChevronRight size={11} />
      {/if}
    </div>
  </button>

  <!-- Expanded body -->
  {#if isExpanded}
    <div
      transition:slide={{ duration: 150 }}
      class="space-y-1.5 border-t border-border/15 px-2 pb-2 pt-1.5"
    >
      <!-- Optional thinking/reasoning text from message metadata -->
      {#if thinkingText}
        <div
          class="rounded glass-inset px-2.5 py-2 text-[11px] italic leading-relaxed text-muted-foreground/80"
        >
          {thinkingText}
        </div>
      {/if}

      <!-- Numbered steps -->
      {#each toolCalls as toolCall, i (toolCall.tool_name + "-" + i)}
        <div class="flex gap-1.5">
          <!-- Step badge (only when multiple calls) -->
          {#if total > 1}
            <div class="mt-[7px] flex-shrink-0">
              <span
                class="flex h-4 w-4 items-center justify-center rounded-full bg-muted/50 font-mono text-[9px] text-muted-foreground/60"
              >
                {i + 1}
              </span>
            </div>
          {/if}

          <!-- Tool card -->
          <div class="min-w-0 flex-1">
            {#if toolCall.status === "pending"}
              {#if isOperator}
                <OperatorApprovalCard
                  {sessionId}
                  messageId={messageId}
                  {toolCall}
                />
              {:else}
                <ApprovalCard
                  {sessionId}
                  messageId={messageId}
                  toolName={toolCall.tool_name}
                  inputPreview={JSON.stringify(toolCall.input, null, 2)}
                />
              {/if}
            {:else if isOperator}
              <OperatorToolCard {toolCall} />
            {:else}
              <BuilderToolCard {toolCall} />
            {/if}
          </div>
        </div>
      {/each}
    </div>
  {/if}
</div>
