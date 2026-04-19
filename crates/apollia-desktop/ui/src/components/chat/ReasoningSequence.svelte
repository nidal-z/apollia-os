<script lang="ts">
  /**
   * Ordered reasoning sequence for an assistant message.
   *
   * Combines thinking trace + normalized tool calls into a vertical stream of
   * `ReasoningCard`s. Pending tool calls still render the dedicated approval
   * cards (unchanged by US-SP42-028 — their refonte is tracked in
   * US-SP42-032). Collapses into an accordion when more than
   * `COLLAPSE_ITEM_THRESHOLD` items are present (B.10, B.33).
   */

  import type { ChatMessageView, ToolCallView } from "$lib/types";
  import { buildReasoningSequence, COLLAPSE_ITEM_THRESHOLD } from "$lib/chat/reasoning";
  import ReasoningCard from "./ReasoningCard.svelte";
  import ApprovalCard from "./ApprovalCard.svelte";
  import OperatorApprovalCard from "./OperatorApprovalCard.svelte";
  import { ChevronDown, ChevronRight } from "lucide-svelte";
  import { t } from "svelte-i18n";
  import { slide } from "svelte/transition";

  interface Props {
    message: Pick<ChatMessageView, "id" | "tool_calls" | "metadata">;
    sessionId: string;
    isOperator: boolean;
  }

  let { message, sessionId, isOperator }: Props = $props();

  const toolCalls = $derived<ToolCallView[]>(message.tool_calls ?? []);
  const pendingCalls = $derived(
    toolCalls.filter(
      (c) => c.status === "pending" || c.status === "authorized",
    ),
  );
  const nonPendingCalls = $derived(
    toolCalls.filter(
      (c) => c.status !== "pending" && c.status !== "authorized",
    ),
  );

  const nonPendingMessage = $derived({
    id: message.id,
    tool_calls: nonPendingCalls,
    metadata: message.metadata,
  });
  const items = $derived(buildReasoningSequence(nonPendingMessage));

  const overflow = $derived(items.length > COLLAPSE_ITEM_THRESHOLD);
  let expandedGroup = $state(false);
  const visibleItems = $derived(
    overflow && !expandedGroup
      ? items.slice(0, COLLAPSE_ITEM_THRESHOLD)
      : items,
  );

  const skin = $derived<"builder" | "operator">(
    isOperator ? "operator" : "builder",
  );
</script>

{#if toolCalls.length > 0 || message.metadata?.thinking_trace}
  <div class="mt-2 space-y-1.5" data-testid="reasoning-sequence">
    <!-- Non-pending items rendered via the unified ReasoningCard -->
    {#each visibleItems as item (item.id)}
      <ReasoningCard {item} {skin} />
    {/each}

    <!-- Collapse-if-too-many toggle -->
    {#if overflow}
      <button
        type="button"
        class="flex w-full items-center gap-1.5 rounded-md px-2 py-1 text-[11px] text-muted-foreground hover:bg-muted/25"
        onclick={() => (expandedGroup = !expandedGroup)}
        aria-expanded={expandedGroup}
      >
        {#if expandedGroup}
          <ChevronDown size={11} />
          {$t("chat.reasoning.group_collapse", {
            default: "Hide steps",
          })}
        {:else}
          <ChevronRight size={11} />
          {$t("chat.reasoning.group_expand", {
            default: "Show {n} more steps",
            values: { n: items.length - COLLAPSE_ITEM_THRESHOLD },
          })}
        {/if}
      </button>
    {/if}

    <!-- Pending approvals keep the dedicated cards (kept for US-SP42-032) -->
    {#each pendingCalls as toolCall, i (toolCall.tool_name + "-pending-" + i)}
      <div transition:slide={{ duration: 150 }}>
        {#if isOperator}
          <OperatorApprovalCard
            {sessionId}
            messageId={message.id}
            {toolCall}
          />
        {:else}
          <ApprovalCard
            {sessionId}
            messageId={message.id}
            toolName={toolCall.tool_name}
            inputPreview={JSON.stringify(toolCall.input, null, 2)}
          />
        {/if}
      </div>
    {/each}
  </div>
{/if}
