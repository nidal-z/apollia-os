<script lang="ts">
  /**
   * Session-scoped host for the conversational plan gate.
   *
   * Owns the "plan-mode" runtime-event subscription for the active chat session
   * (cleaned up in the `$effect`) and renders the review card when the session
   * is awaiting approval. Operator persona gets the compact review; Builder
   * persona gets the detailed one. No card is shown otherwise (clean empty
   * state, no phantom frame).
   */
  import { chatPlanState, startChatPlanListener } from "$lib/stores/chatPlanMode";
  import { uiMode } from "$lib/stores/mode";
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
  const isBuilder = $derived($uiMode === "builder");
</script>

{#if visible}
  <div class="px-4 pb-2" data-testid="chat-plan-host">
    {#if isBuilder}
      <ChatPlanReviewBuilder {sessionId} />
    {:else}
      <ChatPlanReview {sessionId} />
    {/if}
  </div>
{/if}
