<!--
  The scrolling body of a conversation: the message groups, the streaming turn,
  the delegation card, the two inline HITL surfaces, the plan gate, and the
  jump-to-latest button.

  It owns nothing. The conversation holds the thread and the controllers; this
  component is the arrangement, split out because the conversation had grown past
  the module-size rule of `crates/apollia-desktop/ui/AGENTS.md`.
-->
<script lang="ts">
  import { t } from "svelte-i18n";
  import { MessageSquare, Link } from "lucide-svelte";
  import { Spinner } from "$lib/components/ui/progress";
  import { Button } from "$lib/components/ui/button";
  import { uiMode } from "$lib/stores/mode";
  import { contextDrawerOpen } from "$lib/stores/chatLayout";
  import MessageGroup from "./MessageGroup.svelte";
  import StreamingMessage from "./StreamingMessage.svelte";
  import A2ADelegationCard from "./A2ADelegationCard.svelte";
  import ApprovalCard from "./ApprovalCard.svelte";
  import AskUserCard from "./AskUserCard.svelte";
  import ChatPlanHost from "./ChatPlanHost.svelte";
  import ScrollToBottomButton from "./ScrollToBottomButton.svelte";
  import type { MessageGroup as MessageGroupView } from "$lib/chat/groupMessages";
  import type { A2ADelegation } from "./useA2ADelegation.svelte";
  import type { ConversationScroll } from "./useConversationScroll.svelte";
  import type { LiveToolCall } from "./liveToolChain";

  interface Props {
    sessionId: string;
    sessionMode: "libre" | "agent";
    sessionAgentName: string | null;
    sessionStatus: "active" | "processing" | "closed";
    messageGroups: MessageGroupView[];
    /** True when the thread carries no message at all. */
    empty: boolean;
    isStreaming: boolean;
    isProcessing: boolean;
    tokenBuffer: string;
    liveToolChain: LiveToolCall[];
    liveSkin: "builder" | "operator";
    /** True when at least one group quotes a past session. */
    hasCrossSessionRefs: boolean;
    a2a: A2ADelegation;
    scroll: ConversationScroll;
    pendingApproval: {
      sessionId: string;
      messageId: string;
      toolCallId: string;
      toolName: string;
      inputPreview: string;
    } | null;
    pendingUserInput: {
      requestId: string;
      questions: { id: string; question: string; type: "open" | "single_choice" | "multi_choice"; options?: string[]; hint?: string }[];
      context?: string | null;
    } | null;
    onregenerate: (messageId: string) => void;
    onedit: (messageId: string, newContent: string) => void;
  }

  let {
    sessionId,
    sessionMode,
    sessionAgentName,
    sessionStatus,
    messageGroups,
    empty,
    isStreaming,
    isProcessing,
    tokenBuffer,
    liveToolChain,
    liveSkin,
    hasCrossSessionRefs,
    a2a,
    scroll,
    pendingApproval,
    pendingUserInput,
    onregenerate,
    onedit,
  }: Props = $props();
</script>

<div class="relative flex-1 min-h-0">
<div
  bind:this={scroll.container}
  onscroll={() => scroll.onScroll()}
  class="h-full overflow-y-auto px-4 py-5 space-y-5 [&>*]:mx-auto [&>*]:w-full {$contextDrawerOpen
    ? '[&>*]:max-w-[680px]'
    : '[&>*]:max-w-[860px]'}"
  data-testid="chat-messages-list"
>
  {#if empty && !isStreaming && !isProcessing}
    <div class="flex h-full flex-col items-center justify-center gap-2 text-muted-foreground/40">
      <MessageSquare size={28} />
      <p class="text-xs">{$t("chat.first_message_placeholder")}</p>
    </div>
  {:else}
    <!-- SummarizedMessagesBanner moved to ContextDrawer's Memory tab
         - no longer rendered inline in the message list. -->

    {#each messageGroups as group (group.key)}
      {@const firstMsg = group.messages[0]}
      {@const isSingleCrossSession =
        group.messages.length === 1 &&
        firstMsg.role === "assistant" &&
        Boolean(firstMsg.metadata?.cross_session)}
      <div class="relative" data-message-id={group.key} tabindex="-1">
        <MessageGroup
          {group}
          {sessionId}
          agentName={sessionAgentName}
          busy={isProcessing || isStreaming}
          {onregenerate}
          {onedit}
        />
        {#if $uiMode === "builder" && hasCrossSessionRefs && isSingleCrossSession}
          <div
            class="absolute -top-1 -right-1 flex items-center gap-1 rounded-full bg-secondary/20 px-2 py-0.5"
            data-testid="cross-session-badge"
          >
            <Link size={9} class="text-secondary" />
            <span class="text-micro-xs font-medium text-secondary">{$t("chat.past_session")}</span>
          </div>
        {/if}
      </div>
    {/each}

    {#if isStreaming || liveToolChain.length > 0}
      <!-- Streaming turn is isolated in its own component so only this
           subtree re-renders per token. It renders the append-only live
           timeline (reasoning captions + tool rows in arrival order) plus
           the streaming answer, so nothing is torn down when a tool call or
           approval card appears. Rendered while streaming OR while any tool
           row is live (a tool can precede the first token). -->
      <StreamingMessage
        text={tokenBuffer}
        {sessionMode}
        agentName={sessionAgentName}
        toolChain={liveToolChain}
        skin={liveSkin}
      />
    {/if}

    {#if a2a.active}
      <A2ADelegationCard
        target={a2a.active.target}
        skillId={a2a.active.skill_id}
        elapsed={a2a.elapsed}
        steps={a2a.steps}
        guardMessage={a2a.guardMessage}
      />
    {/if}

    {#if pendingApproval}
      <div class="flex flex-col items-start gap-1" data-testid="chat-approval-inline">
        <div class="w-full">
          <!-- Key on the approval identity so back-to-back HITL prompts each
               mount a fresh card, never inheriting the previous card's busy
               (greyed) state. -->
          {#key pendingApproval.toolCallId}
            <ApprovalCard
              sessionId={pendingApproval.sessionId}
              messageId={pendingApproval.messageId}
              toolCallId={pendingApproval.toolCallId}
              toolName={pendingApproval.toolName}
              inputPreview={pendingApproval.inputPreview}
            />
          {/key}
        </div>
        <Button variant="ghost" size="sm"
          type="button"
          class="text-caption text-primary hover:underline"
          onclick={() => {
            const pa = pendingApproval;
            if (!pa) return;
            const id = `chat:${pa.sessionId}:${pa.messageId}:${pa.toolCallId}`;
            if (typeof window !== "undefined") {
              history.replaceState(null, "", `#inbox?item=${encodeURIComponent(id)}`);
            }
            // Lazy import keeps the chat bundle clean.
            import("$lib/stores/navigation").then((m) => m.navigateTo("inbox"));
          }}
          data-testid="chat-approval-open-inbox"
        >
          {$t("inbox.open_in_inbox")} →
        </Button>
      </div>
    {/if}

    {#if pendingUserInput}
      <div class="flex justify-start" data-testid="chat-ask-user-inline">
        <div class="w-full">
          <AskUserCard
            requestId={pendingUserInput.requestId}
            questions={pendingUserInput.questions}
            context={pendingUserInput.context}
          />
        </div>
      </div>
    {/if}

    <!-- Plan gate flows below the assistant message in normal document
         flow (inside the scroll), so the "Proposed plan" card sits under
         the streamed turn instead of overlapping it. -->
    {#if sessionId && sessionStatus !== "closed"}
      <ChatPlanHost {sessionId} />
    {/if}

    {#if isProcessing && sessionMode === "agent"}
      <div class="flex justify-start" data-testid="chat-agent-loading">
        <div class="flex items-center gap-1.5 rounded-lg bg-muted/40 px-3 py-1.5 text-caption text-muted-foreground">
          <Spinner size={11} />
          <span>{$t("chat.agent_processing")}</span>
        </div>
      </div>
    {/if}

    {#if isProcessing && sessionMode === "libre"}
      <div class="flex justify-start">
        <div class="flex items-center gap-1.5 rounded-lg bg-muted/40 px-3 py-1.5 text-caption text-muted-foreground">
          <Spinner size={11} />
          <span>{$t("chat.thinking")}</span>
        </div>
      </div>
    {/if}
  {/if}
</div>
<ScrollToBottomButton
  visible={scroll.showJump}
  unreadCount={scroll.unread}
  onclick={() => scroll.jumpToLatest()}
/>
</div>
