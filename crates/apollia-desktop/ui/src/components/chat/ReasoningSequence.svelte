<script lang="ts">
  /**
   * Ordered reasoning sequence for an assistant message.
   *
   * Combines thinking trace + normalized tool calls into a vertical stream of
   * `ReasoningCard`s. Pending tool calls still render the dedicated approval
   * cards (unchanged by their refonte is tracked in
   *). Collapses into an accordion when more than
   * `COLLAPSE_ITEM_THRESHOLD` items are present.
   */

  import type { ChatMessageView, ToolCallView } from "$lib/types";
  import { buildReasoningSequence, COLLAPSE_ITEM_THRESHOLD } from "$lib/chat/reasoning";
  import ReasoningCard from "./ReasoningCard.svelte";
  import ApprovalCard from "./ApprovalCard.svelte";
  import OperatorApprovalCard from "./OperatorApprovalCard.svelte";
  import { ChevronDown, ChevronRight } from "lucide-svelte";
  import { t } from "svelte-i18n";
  import { slide, fly } from "svelte/transition";
  import { Button } from "$lib/components/ui/button";

  interface Props {
    message: Pick<ChatMessageView, "id" | "tool_calls" | "metadata">;
    sessionId: string;
    isOperator: boolean;
    content?: string;
    /**
     * Which part of the sequence to render. `trace` renders only the reasoning
     * captions and tool rows (meant to live inside a collapsed `ActivityStrip`);
     * `approvals` renders only the pending HITL cards (which must stay visible,
     * never hidden in a collapsed strip); `all` renders both (default).
     */
    section?: "all" | "trace" | "approvals";
  }

  let {
    message,
    sessionId,
    isOperator,
    content,
    section = "all",
  }: Props = $props();

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
  const items = $derived(buildReasoningSequence(nonPendingMessage, content));

  // cap visible reasoning items, paginate by 30 on demand.
  // Persist `visibleCount` per-message in sessionStorage so scroll-back through
  // a conversation preserves the user's pagination state within the tab.
  const PAGE_SIZE = 30;
  const storageKey = $derived(`apollia.reasoning.visibleCount.${message.id}`);

  function loadVisibleCount(): number {
    if (typeof sessionStorage === "undefined") return COLLAPSE_ITEM_THRESHOLD;
    const raw = sessionStorage.getItem(storageKey);
    if (!raw) return COLLAPSE_ITEM_THRESHOLD;
    const n = Number.parseInt(raw, 10);
    return Number.isFinite(n) && n > 0 ? n : COLLAPSE_ITEM_THRESHOLD;
  }

  let visibleCount = $state<number>(loadVisibleCount());

  const overflow = $derived(items.length > visibleCount);
  const hiddenCount = $derived(Math.max(items.length - visibleCount, 0));
  const visibleItems = $derived(
    overflow ? items.slice(0, visibleCount) : items,
  );

  function showMore(): void {
    visibleCount = Math.min(visibleCount + PAGE_SIZE, items.length);
    persistVisibleCount();
  }

  function collapse(): void {
    visibleCount = COLLAPSE_ITEM_THRESHOLD;
    persistVisibleCount();
  }

  function persistVisibleCount(): void {
    if (typeof sessionStorage === "undefined") return;
    try {
      sessionStorage.setItem(storageKey, String(visibleCount));
    } catch {
      // quota / private mode - ignore.
    }
  }

  const skin = $derived<"builder" | "operator">(
    isOperator ? "operator" : "builder",
  );
</script>

{#if (section !== "approvals" && items.length > 0) || (section !== "trace" && pendingCalls.length > 0)}
  <div class="space-y-1.5 {section === 'all' ? 'mt-2' : ''}" data-testid="reasoning-sequence">
    <!-- Non-pending items rendered via the unified ReasoningCard. Each block
         flies in as it appears, so live reasoning streams in progressively. -->
    {#if section !== "approvals"}
    {#each visibleItems as item (item.id)}
      <div in:fly={{ x: -12, duration: 260 }}>
        <ReasoningCard {item} {skin} {sessionId} />
      </div>
    {/each}

    <!-- Pagination: show +30 at a time, then collapse back to initial view. -->
    {#if overflow}
      <Button variant="ghost" size="sm"
        type="button"
        class="flex w-full items-center gap-1.5 rounded-md px-2 py-1 text-[11px] text-muted-foreground hover:bg-muted/25"
        onclick={showMore}
        data-testid="reasoning-show-more"
      >
        <ChevronRight size={11} />
        {$t("chat.reasoning.group_expand", {
          default: "Show {n} more steps",
          values: { n: Math.min(hiddenCount, PAGE_SIZE) },
        })}
      </Button>
    {:else if visibleCount > COLLAPSE_ITEM_THRESHOLD && items.length > COLLAPSE_ITEM_THRESHOLD}
      <Button variant="ghost" size="sm"
        type="button"
        class="flex w-full items-center gap-1.5 rounded-md px-2 py-1 text-[11px] text-muted-foreground hover:bg-muted/25"
        onclick={collapse}
        data-testid="reasoning-collapse"
      >
        <ChevronDown size={11} />
        {$t("chat.reasoning.group_collapse", {
          default: "Hide steps",
        })}
      </Button>
    {/if}
    {/if}

    <!-- Pending approvals keep the dedicated cards. They stay outside any
         collapsed activity strip so a HITL request is never hidden. -->
    {#if section !== "trace"}
    {#each pendingCalls as toolCall, i (toolCall.tool_name + "-pending-" + i)}
      <div transition:slide={{ duration: 150 }}>
        {#if isOperator}
          <OperatorApprovalCard
            {sessionId}
            messageId={message.id}
            toolCallId={toolCall.tool_name}
            {toolCall}
          />
        {:else}
          <ApprovalCard
            {sessionId}
            messageId={message.id}
            toolCallId={toolCall.tool_name}
            toolName={toolCall.tool_name}
            inputPreview={JSON.stringify(toolCall.input, null, 2)}
          />
        {/if}
      </div>
    {/each}
    {/if}
  </div>
{/if}
