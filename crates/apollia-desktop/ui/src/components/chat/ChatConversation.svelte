<script lang="ts">
  import { onMount, onDestroy, tick } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { t } from "svelte-i18n";
  import { X, Loader2, Bot, MessageSquare, Settings2, XCircle, Link } from "lucide-svelte";
  import { currentSession, chatTokenBuffer, useUserMemory, memoryEntryCount, chatConversationStats } from "$lib/stores/chat";
  import { uiMode } from "$lib/stores/mode";
  import { Badge } from "$lib/components/ui/badge";
  import type { ChatSessionDetail, ChatMessageView, ConversationStatsView, UserMemoryProfileView } from "$lib/types";
  import ChatMessageBubble from "./ChatMessageBubble.svelte";
  import ChatInput from "./ChatInput.svelte";
  import StreamingText from "./StreamingText.svelte";
  import ChatConfigPanel from "./ChatConfigPanel.svelte";
  import ContextIndicator from "./ContextIndicator.svelte";
  import SummarizedMessagesBanner from "./SummarizedMessagesBanner.svelte";

  interface Props {
    sessionId: string;
    onclose: () => void;
    /** When true, hide the header bar (used for embedded contexts like onboarding). */
    embedded?: boolean;
    /** When true, hide the config button in the header. */
    hideConfig?: boolean;
  }

  let { sessionId, onclose, embedded = false, hideConfig = false }: Props = $props();

  let messages = $state<ChatMessageView[]>([]);
  let sessionMode = $state<"libre" | "agent">("libre");
  let sessionAgentName = $state<string | null>(null);
  let sessionStatus = $state<"active" | "processing" | "closed">("active");
  let isStreaming = $state(false);
  let isProcessing = $state(false);
  let loading = $state(true);
  let loadError = $state<string | null>(null);
  let messagesContainer = $state<HTMLDivElement | undefined>(undefined);
  let userScrolledUp = $state(false);
  let tokenBuffer = $state("");
  let configOpen = $state(false);
  let sessionDetail = $state<ChatSessionDetail | null>(null);
  let conversationStats = $state<ConversationStatsView | null>(null);

  const headerTitle = $derived(
    sessionMode === "agent" && sessionAgentName
      ? sessionAgentName
      : $t("chat.mode_libre"),
  );

  const inputDisabled = $derived(
    isProcessing || isStreaming || sessionStatus === "closed",
  );

  const summaryText = $derived.by(() => {
    const prompt = sessionDetail?.system_prompt ?? "";
    const marker = "Previous context summary:\n";
    const idx = prompt.indexOf(marker);
    if (idx < 0) return null;
    return prompt.slice(idx + marker.length).trim() || null;
  });

  const summarizedCount = $derived(conversationStats?.summarized_count ?? 0);

  const hasCrossSessionRefs = $derived(
    (conversationStats?.cross_sessions_referenced ?? 0) > 0,
  );

  const avatarHue = $derived.by(() => {
    if (!sessionAgentName) return 220;
    let hash = 0;
    for (let i = 0; i < sessionAgentName.length; i++) {
      hash = sessionAgentName.charCodeAt(i) + ((hash << 5) - hash);
    }
    return Math.abs(hash) % 360;
  });

  let unlistenToken: UnlistenFn | undefined;
  let unlistenChanged: UnlistenFn | undefined;
  let activeToolName = $state<string | null>(null);

  onMount(async () => {
    await loadSession();

    unlistenToken = await listen<{ session_id: string; message_id: string; token: string }>(
      "chat-token",
      (event) => {
        if (event.payload.session_id !== sessionId) return;
        activeToolName = null;
        tokenBuffer += event.payload.token;
        isStreaming = true;
        isProcessing = false;
        scrollToBottom();
      },
    );

    unlistenChanged = await listen<{ category: string; event_type: string; payload: Record<string, unknown> }>(
      "runtime-event",
      (event) => {
        if (event.payload.category !== "chat-changed") return;
        const evt = event.payload;

        if (evt.event_type === "ChatToolCallStarted") {
          const p = evt.payload as { session_id?: string; tool_name?: string };
          if (p.session_id === sessionId) {
            activeToolName = p.tool_name ?? null;
            scrollToBottom();
          }
          return;
        }
        if (evt.event_type === "ChatToolCallCompleted") {
          const p = evt.payload as { session_id?: string };
          if (p.session_id === sessionId) activeToolName = null;
          return;
        }
        if (evt.event_type === "ChatResponseCompleted") {
          void finalizeStreaming();
          return;
        }
        void refreshSession();
      },
    );
  });

  // Re-load when sessionId prop changes (switching conversations)
  let previousSessionId = $state(sessionId);
  $effect(() => {
    if (sessionId !== previousSessionId) {
      previousSessionId = sessionId;
      isStreaming = false;
      isProcessing = false;
      tokenBuffer = "";
      activeToolName = null;
      messages = [];
      void loadSession();
    }
  });

  onDestroy(() => {
    unlistenToken?.();
    unlistenChanged?.();
    currentSession.set(null);
    chatTokenBuffer.set("");
  });

  async function loadSession(): Promise<void> {
    loading = true; loadError = null;
    try {
      const detail = await invoke<ChatSessionDetail>("get_chat_session", { sessionId });
      applySessionDetail(detail);
    } catch (err: unknown) { loadError = err instanceof Error ? err.message : String(err); }
    finally { loading = false; await tick(); scrollToBottom(true); }
  }

  async function refreshSession(): Promise<void> {
    try {
      const detail = await invoke<ChatSessionDetail>("get_chat_session", { sessionId });
      applySessionDetail(detail); scrollToBottom();
    } catch { /* Session may have been deleted */ }
  }

  async function finalizeStreaming(): Promise<void> {
    await new Promise((r) => setTimeout(r, 80));
    isStreaming = false; isProcessing = false; tokenBuffer = ""; chatTokenBuffer.set("");
    try {
      const detail = await invoke<ChatSessionDetail>("get_chat_session", { sessionId });
      applySessionDetail(detail); scrollToBottom();
    } catch { /* Session may have been deleted */ }
  }

  function applySessionDetail(detail: ChatSessionDetail): void {
    messages = detail.messages ?? [];
    sessionMode = detail.mode;
    sessionAgentName = detail.agent_name;
    sessionStatus = detail.status;
    sessionDetail = detail;
    isProcessing = detail.status === "processing";
    currentSession.set(detail);
    void loadConversationStats();
  }

  async function loadConversationStats(): Promise<void> {
    try {
      const stats = await invoke<ConversationStatsView>("get_conversation_stats", { sessionId });
      conversationStats = stats;
      chatConversationStats.set(stats);
    } catch {
      conversationStats = null;
    }
    try {
      const profile = await invoke<UserMemoryProfileView>("get_user_memory_profile");
      memoryEntryCount.set(profile.stats.total);
    } catch {
      memoryEntryCount.set(0);
    }
  }

  function scrollToBottom(force = false): void {
    if (!force && userScrolledUp) return;
    requestAnimationFrame(() => {
      if (messagesContainer) {
        messagesContainer.scrollTo({ top: messagesContainer.scrollHeight, behavior: force ? "instant" : "smooth" });
      }
    });
  }

  function handleScroll(): void {
    if (!messagesContainer) return;
    const { scrollTop, scrollHeight, clientHeight } = messagesContainer;
    userScrolledUp = scrollHeight - scrollTop - clientHeight > 60;
  }

  async function handleSend(content: string): Promise<void> {
    const tempMsg: ChatMessageView = {
      id: `temp-${Date.now()}`, role: "user", content,
      tool_calls: null, tool_name: null,
      seq: (messages ?? []).length, created_at: new Date().toISOString(),
    };
    messages = [...(messages ?? []), tempMsg];
    isProcessing = true; tokenBuffer = ""; chatTokenBuffer.set("");
    await tick(); scrollToBottom(true);

    try {
      await invoke<string>("send_chat_message", { sessionId, content });
    } catch (err: unknown) {
      isProcessing = false;
      const errMsg: ChatMessageView = {
        id: `error-${Date.now()}`, role: "system",
        content: err instanceof Error ? err.message : String(err),
        tool_calls: null, tool_name: null,
        seq: (messages ?? []).length, created_at: new Date().toISOString(),
      };
      messages = [...(messages ?? []), errMsg]; scrollToBottom();
    }
  }

  async function handleCloseSession(): Promise<void> {
    try { await invoke("close_chat_session", { sessionId }); }
    catch (err: unknown) { console.warn("close_chat_session IPC not available:", err); }
    void refreshSession();
  }
</script>

<div class="flex h-full flex-col" data-testid="chat-conversation">
  <!-- Header — slim bar (hidden in embedded mode) -->
  {#if !embedded}
  <div class="flex items-center justify-between border-b border-border/30 px-4 py-2.5">
    <div class="flex items-center gap-2.5">
      {#if sessionMode === "agent"}
        <Bot size={15} class="text-primary" />
      {:else}
        <MessageSquare size={15} class="text-muted-foreground" />
      {/if}
      <span class="text-[13px] font-medium">{headerTitle}</span>
      {#if sessionStatus === "closed"}
        <Badge variant="secondary" class="text-[9px] px-1.5 py-0">{$t("chat.status_closed")}</Badge>
      {:else if sessionStatus === "processing"}
        <span class="flex items-center gap-1 text-[11px] text-primary/70">
          <Loader2 size={11} class="animate-spin" />
          {$t("chat.thinking")}
        </span>
      {/if}
    </div>
    <div class="flex items-center gap-0.5">
      {#if !hideConfig}
      <button
        onclick={() => (configOpen = true)}
        class="h-7 w-7 inline-flex items-center justify-center rounded-md text-muted-foreground/60 hover:text-foreground hover:bg-muted/40 transition-colors"
        aria-label={$t("chat.config_title")}
        data-testid="chat-config-button"
      >
        <Settings2 size={14} />
      </button>
      {/if}
      {#if sessionStatus !== "closed"}
        <button
          onclick={handleCloseSession}
          class="h-7 w-7 inline-flex items-center justify-center rounded-md text-muted-foreground/60 hover:text-warning hover:bg-warning/10 transition-colors"
          aria-label={$t("chat.close_session")}
          data-testid="chat-close-session-button"
        >
          <XCircle size={14} />
        </button>
      {/if}
      <button
        onclick={onclose}
        class="h-7 w-7 inline-flex items-center justify-center rounded-md text-muted-foreground/60 hover:text-foreground hover:bg-muted/40 transition-colors"
        aria-label="Close"
        data-testid="chat-close-button"
      >
        <X size={14} />
      </button>
    </div>
  </div>
  {/if}

  <!-- Context indicator -->
  <ContextIndicator
    memoryEntryCount={$memoryEntryCount}
    isInjected={$useUserMemory && (conversationStats?.user_memory_injected ?? false)}
  />

  <!-- Messages -->
  {#if loading}
    <div class="flex flex-1 items-center justify-center">
      <Loader2 size={16} class="animate-spin text-muted-foreground" />
    </div>
  {:else if loadError}
    <div class="flex flex-1 items-center justify-center px-6">
      <p class="text-xs text-destructive">{loadError}</p>
    </div>
  {:else}
    <div
      bind:this={messagesContainer}
      onscroll={handleScroll}
      class="flex-1 overflow-y-auto px-4 py-4 space-y-3"
    >
      {#if (messages ?? []).length === 0 && !isStreaming && !isProcessing}
        <div class="flex h-full flex-col items-center justify-center gap-2 text-muted-foreground/40">
          <MessageSquare size={28} />
          <p class="text-xs">{$t("chat.first_message_placeholder")}</p>
        </div>
      {:else}
        {#if summarizedCount > 0 && summaryText}
          <SummarizedMessagesBanner
            summarizedCount={summarizedCount}
            summaryText={summaryText}
          />
        {/if}

        {#each messages ?? [] as message (message.id)}
          <div class="relative">
            <ChatMessageBubble {message} {sessionId} />
            {#if $uiMode === "builder" && hasCrossSessionRefs && message.role === "assistant" && message.metadata?.cross_session}
              <div
                class="absolute -top-1 -right-1 flex items-center gap-1 rounded-full bg-[#7c5fd6]/20 px-2 py-0.5"
                data-testid="cross-session-badge"
              >
                <Link size={9} class="text-[#7c5fd6]" />
                <span class="text-[9px] font-medium text-[#7c5fd6]">{$t("chat.past_session")}</span>
              </div>
            {/if}
          </div>
        {/each}

        {#if isStreaming && sessionMode === "libre"}
          <div class="flex justify-start" data-testid="chat-message-streaming">
            <div class="max-w-[72%] rounded-2xl rounded-bl-sm bg-card/80 border border-border/30 px-3.5 py-2.5 text-[13px] text-foreground">
              <StreamingText text={tokenBuffer} />
            </div>
          </div>
        {/if}

        {#if activeToolName}
          <div class="flex justify-start" data-testid="chat-tool-executing">
            <div class="flex items-center gap-1.5 rounded-lg bg-muted/40 px-3 py-1.5 text-[11px] text-muted-foreground">
              <Loader2 size={11} class="animate-spin" />
              <span>{$t("chat.tool_executing", { values: { tool: activeToolName } })}</span>
            </div>
          </div>
        {/if}

        {#if isProcessing && sessionMode === "agent"}
          <div class="flex justify-start" data-testid="chat-agent-loading">
            <div class="flex items-center gap-1.5 rounded-lg bg-muted/40 px-3 py-1.5 text-[11px] text-muted-foreground">
              <Loader2 size={11} class="animate-spin" />
              <span>{$t("chat.agent_processing")}</span>
            </div>
          </div>
        {/if}

        {#if isProcessing && sessionMode === "libre"}
          <div class="flex justify-start">
            <div class="flex items-center gap-1.5 rounded-lg bg-muted/40 px-3 py-1.5 text-[11px] text-muted-foreground">
              <Loader2 size={11} class="animate-spin" />
              <span>{$t("chat.thinking")}</span>
            </div>
          </div>
        {/if}
      {/if}
    </div>
  {/if}

  <ChatInput disabled={inputDisabled} onsend={handleSend} />
</div>

{#if !embedded}
<ChatConfigPanel
  open={configOpen}
  session={sessionDetail}
  onclose={() => (configOpen = false)}
  onupdated={() => void refreshSession()}
/>
{/if}
