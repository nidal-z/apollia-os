<script lang="ts">
  /**
   * Ordered reasoning sequence for an assistant message, rendered one slice at a
   * time via the `section` prop:
   *
   * - `reasoning` renders the thinking / rationale trace as flat narrated
   *   captions (a gradient-stroked brain marker + italic muted prose, no
   *   per-item toggle). Meant to live inside a collapsed `ActivityStrip`.
   * - `tools` renders the finalized tool calls as expandable `ReasoningCard`
   *   rows, visible in the thread flow so each per-tool body stays reachable.
   * - `approvals` renders the pending HITL cards, which stay visible and are
   *   never hidden inside a collapsed strip.
   */

  import type { ChatMessageView, ToolCallView } from "$lib/types";
  import { buildReasoningSequence, COLLAPSE_ITEM_THRESHOLD } from "$lib/chat/reasoning";
  import ReasoningCard from "./ReasoningCard.svelte";
  import ApprovalCard from "./ApprovalCard.svelte";
  import OperatorApprovalCard from "./OperatorApprovalCard.svelte";
  import MarkdownContent from "$lib/components/ui/markdown/MarkdownContent.svelte";
  import { Brain, ChevronDown, ChevronRight } from "lucide-svelte";
  import { t } from "svelte-i18n";
  import { slide, fly } from "svelte/transition";
  import { Button } from "$lib/components/ui/button";

  interface Props {
    message: Pick<ChatMessageView, "id" | "tool_calls" | "metadata">;
    sessionId: string;
    isOperator: boolean;
    content?: string;
    /**
     * Which slice of the turn to render. See the module doc-comment: `reasoning`
     * (flat captions in the strip), `tools` (expandable rows in the flow), or
     * `approvals` (always-visible HITL cards).
     */
    section: "reasoning" | "tools" | "approvals";
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
  const items = $derived(buildReasoningSequence(nonPendingMessage, content));

  // Split the sequence by kind: thoughts become flat captions, everything else
  // (tool calls, retry chains, citations) becomes an expandable card row.
  const reasoningItems = $derived(
    items.filter((i) => i.kind === "thinking" || i.kind === "rationale"),
  );
  const toolItems = $derived(
    items.filter((i) => i.kind !== "thinking" && i.kind !== "rationale"),
  );

  // Cap visible tool rows, paginate by 30 on demand. Persist `visibleCount`
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

  const overflow = $derived(toolItems.length > visibleCount);
  const hiddenCount = $derived(Math.max(toolItems.length - visibleCount, 0));
  const visibleTools = $derived(
    overflow ? toolItems.slice(0, visibleCount) : toolItems,
  );

  function showMore(): void {
    visibleCount = Math.min(visibleCount + PAGE_SIZE, toolItems.length);
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

{#if section === "reasoning"}
  {#if reasoningItems.length > 0}
    <!-- Flat narrated captions: a quiet, single-log reading of the agent's
         thoughts. No per-item chevron, label, or left rule, so nothing reads as
         a second collapsible system inside the strip. -->
    <div class="flex flex-col gap-0.5" data-testid="reasoning-sequence">
      {#each reasoningItems as item (item.id)}
        {#if item.kind === "thinking" || item.kind === "rationale"}
          <div
            class="flex items-start gap-2.5 py-1"
            data-testid="reasoning-thought"
            in:fly={{ x: -12, duration: 260 }}
          >
            <span class="tb-think-ico mt-[3px] flex-none" aria-hidden="true">
              <Brain size={13} />
            </span>
            <div
              class="reasoning-caption min-w-0 flex-1 text-[12.5px] italic leading-relaxed text-muted-foreground"
            >
              <MarkdownContent content={item.content} />
            </div>
          </div>
        {/if}
      {/each}
    </div>
  {/if}
{:else if section === "tools"}
  {#if toolItems.length > 0}
    <!-- Tool calls visible in the thread flow. Each row expands to its bespoke
         per-tool body (operator abstraction / builder raw) via ReasoningCard. -->
    <div class="space-y-1.5" data-testid="reasoning-sequence">
      {#each visibleTools as item (item.id)}
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
      {:else if visibleCount > COLLAPSE_ITEM_THRESHOLD && toolItems.length > COLLAPSE_ITEM_THRESHOLD}
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
