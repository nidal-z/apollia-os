<script lang="ts">
  /**
   * Generic approval item wrapper (US-SP42-053, finding A.6.6).
   *
   * Bridges the legacy `<ApprovalCard>` (HITL) and the chat tool approval
   * renderer under a single `InboxItem` surface. Keeps the thin kind-branch
   * the Inbox layout needs without duplicating logic across each caller.
   */
  import type { InboxItem } from "../inbox/types";
  import ApprovalCard from "./ApprovalCard.svelte";
  import ChatApprovalCard from "../chat/ApprovalCard.svelte";

  interface Props {
    item: InboxItem;
  }

  let { item }: Props = $props();
</script>

{#if item.kind === "task"}
  <ApprovalCard approval={item.source} />
{:else}
  <ChatApprovalCard
    sessionId={item.source.sessionId}
    messageId={item.source.messageId}
    toolName={item.source.toolName}
    inputPreview={item.source.inputPreview}
  />
{/if}
