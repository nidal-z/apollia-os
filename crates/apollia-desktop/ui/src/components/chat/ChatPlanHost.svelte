<script lang="ts">
  /**
   * Session-scoped host for the conversational plan gate.
   *
   * Owns the "plan-mode" runtime-event subscription for the active chat session
   * (cleaned up in the `$effect`) and renders the review card when the session
   * is awaiting approval. The default view gets the compact review; the developer
   * mode gets the detailed one. No card is shown otherwise (clean empty
   * state, no phantom frame).
   */
  import { chatPlanState, startChatPlanListener } from "$lib/stores/chatPlanMode";
  import { developerMode } from "$lib/stores/developerMode";
  import ChatPlanReview from "./ChatPlanReview.svelte";
  import ChatPlanReviewBuilder from "./ChatPlanReviewBuilder.svelte";

  interface Props {
    sessionId: string;
  }

  let { sessionId }: Props = $props();

  $effect(() => {
    // Re-subscribe whenever the active session changes.
    const session = sessionId;
    let unlisten: (() => void) | null = null;
    let cancelled = false;
    void startChatPlanListener(session).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  });

  const state = $derived($chatPlanState);
  const visible = $derived(
    state.status === "pending_approval" &&
      state.phase === "awaiting_approval" &&
      state.sessionId === sessionId,
  );
  const detailed = $derived($developerMode);
</script>

{#if visible}
  <div class="px-4 pb-2" data-testid="chat-plan-host">
    {#if detailed}
      <ChatPlanReviewBuilder {sessionId} />
    {:else}
      <ChatPlanReview {sessionId} />
    {/if}
  </div>
{/if}
