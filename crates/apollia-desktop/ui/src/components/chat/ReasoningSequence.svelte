<script lang="ts">
  /**
   * Ordered reasoning sequence for an assistant message, rendered one slice at a
   * time via the `section` prop:
   *
   * - `timeline` renders the turn in the order it happened: thought, action,
   *   thought, action, each row expandable to its details. Splitting it into a
   *   thoughts block and a tools block, as an earlier revision did, threw away
   *   the ordering that `buildReasoningSequence` had just reconstructed, and
   *   with it the only readable account of how the agent reached its answer.
   * - `approvals` renders the pending HITL cards, which stay visible and are
   *   never hidden inside a collapsed strip.
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
     * Which slice of the turn to render. See the module doc-comment:
     * `timeline` (the ordered thought/action rows) or `approvals`
     * (always-visible HITL cards).
     */
    section: "timeline" | "approvals";
  }

  let { message, sessionId, isOperator, content, section }: Props = $props();

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
  // The sequence is rendered as built: thoughts and actions interleaved in the
  // order the ReAct loop produced them.
  const items = $derived(buildReasoningSequence(nonPendingMessage, content));

  // Cap visible rows, paginate by 30 on demand. Persist `visibleCount`
  // per-message in sessionStorage so scroll-back preserves pagination state.
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

{#if section === "timeline"}
  {#if items.length > 0}
    <!-- The turn in the order it happened. Thought rows and tool rows share the
         same shape; each expands to its own detail (the narrated thought, or
         the bespoke per-tool body: operator abstraction / builder raw). -->
    <div class="chat-flow-tools" data-testid="reasoning-sequence">
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
    </div>
  {/if}
{:else if pendingCalls.length > 0}
  <!-- Pending approvals keep the dedicated cards. They stay outside any
       collapsed activity strip so a HITL request is never hidden. -->
  <div class="space-y-1.5" data-testid="reasoning-sequence">
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
  </div>
{/if}
