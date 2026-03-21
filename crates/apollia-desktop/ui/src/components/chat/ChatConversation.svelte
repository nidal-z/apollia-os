<script lang="ts">
  import { onMount, onDestroy, tick } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { t } from "svelte-i18n";
  import { X, Loader2, Bot, MessageSquare, Settings2 } from "lucide-svelte";
  import { currentSession, chatTokenBuffer } from "$lib/stores/chat";
  import { Badge } from "$lib/components/ui/badge";
  import type { ChatSessionDetail, ChatMessageView } from "$lib/types";
  import ChatMessageBubble from "./ChatMessageBubble.svelte";
  import ChatInput from "./ChatInput.svelte";
  import StreamingText from "./StreamingText.svelte";
  import ChatConfigPanel from "./ChatConfigPanel.svelte";

  interface Props {
    sessionId: string;
    onclose: () => void;
  }

  let { sessionId, onclose }: Props = $props();

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

  const headerTitle = $derived(
    sessionMode === "agent" && sessionAgentName
      ? sessionAgentName
      : $t("chat.mode_libre"),
  );

  const headerIcon = $derived(sessionMode === "agent" ? Bot : MessageSquare);

  const inputDisabled = $derived(
    isProcessing || isStreaming || sessionStatus === "closed",
  );

  let unlistenToken: UnlistenFn | undefined;
  let unlistenChanged: UnlistenFn | undefined;

  /** Name of the tool currently being executed (null = no tool running). */
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

        // Tool call lifecycle — show/hide tool indicator during ReAct loop
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
          if (p.session_id === sessionId) {
            activeToolName = null;
          }
          return;
        }

        // ChatResponseCompleted signals the end of streaming — transition
        // gracefully from streamed tokens to persisted message.
        if (evt.event_type === "ChatResponseCompleted") {
          void finalizeStreaming();
          return;
        }

        // Other chat-changed events (session created/closed, approval, etc.)
        void refreshSession();
      },
    );
  });

  onDestroy(() => {
    unlistenToken?.();
    unlistenChanged?.();
    currentSession.set(null);
    chatTokenBuffer.set("");
  });

  async function loadSession(): Promise<void> {
    loading = true;
    loadError = null;
    try {
      const detail = await invoke<ChatSessionDetail>("get_chat_session", { sessionId });
      applySessionDetail(detail);
    } catch (err: unknown) {
      loadError = err instanceof Error ? err.message : String(err);
    } finally {
      loading = false;
      await tick();
      scrollToBottom(true);
    }
  }

  async function refreshSession(): Promise<void> {
    try {
      const detail = await invoke<ChatSessionDetail>("get_chat_session", { sessionId });
      applySessionDetail(detail);
      scrollToBottom();
    } catch {
      // Session may have been deleted — close view
    }
  }

  /// Graceful transition from streaming to persisted state.
  /// Waits a short tick so the last streamed tokens render, then
  /// refreshes the session from SQLite (which now has the full message).
  async function finalizeStreaming(): Promise<void> {
    // Small delay so the user sees the last tokens before the UI swaps
    // from the streaming bubble to the persisted message bubble.
    await new Promise((r) => setTimeout(r, 80));
    isStreaming = false;
    isProcessing = false;
    tokenBuffer = "";
    chatTokenBuffer.set("");

    try {
      const detail = await invoke<ChatSessionDetail>("get_chat_session", { sessionId });
      applySessionDetail(detail);
      scrollToBottom();
    } catch {
      // Session may have been deleted
    }
  }

  function applySessionDetail(detail: ChatSessionDetail): void {
    messages = detail.messages ?? [];
    sessionMode = detail.mode;
    sessionAgentName = detail.agent_name;
    sessionStatus = detail.status;
    sessionDetail = detail;
    isProcessing = detail.status === "processing";
    currentSession.set(detail);
  }

  function scrollToBottom(force = false): void {
    if (!force && userScrolledUp) return;
    requestAnimationFrame(() => {
      if (messagesContainer) {
        messagesContainer.scrollTo({
          top: messagesContainer.scrollHeight,
          behavior: force ? "instant" : "smooth",
        });
      }
    });
  }

  function handleScroll(): void {
    if (!messagesContainer) return;
    const { scrollTop, scrollHeight, clientHeight } = messagesContainer;
    const threshold = 60;
    userScrolledUp = scrollHeight - scrollTop - clientHeight > threshold;
  }

  async function handleSend(content: string): Promise<void> {
    const tempMsg: ChatMessageView = {
      id: `temp-${Date.now()}`,
      role: "user",
      content,
      tool_calls: null,
      tool_name: null,
      seq: (messages ?? []).length,
      created_at: new Date().toISOString(),
    };
    messages = [...(messages ?? []), tempMsg];
    isProcessing = true;
    tokenBuffer = "";
    chatTokenBuffer.set("");

    await tick();
    scrollToBottom(true);

    try {
      await invoke<string>("send_chat_message", { sessionId, content });
    } catch (err: unknown) {
      isProcessing = false;
      const errContent = err instanceof Error ? err.message : String(err);
      const errMsg: ChatMessageView = {
        id: `error-${Date.now()}`,
        role: "system",
        content: errContent,
        tool_calls: null,
        tool_name: null,
        seq: (messages ?? []).length,
        created_at: new Date().toISOString(),
      };
      messages = [...(messages ?? []), errMsg];
      scrollToBottom();
    }
  }
</script>

<div class="flex h-full flex-col" data-testid="chat-conversation">
  <!-- Header -->
  <div class="flex items-center justify-between glass-surface border-b glass-border-subtle px-4 py-3">
    <div class="flex items-center gap-3">
      <div class="flex h-8 w-8 items-center justify-center rounded-lg glass-inset">
        <svelte:component this={headerIcon} class="h-4 w-4 text-primary" />
      </div>
      <div class="flex items-center gap-2">
        <h2 class="text-sm font-semibold">{headerTitle}</h2>
        <Badge
          variant={sessionMode === "agent" ? "default" : "secondary"}
          class="text-[10px] px-1.5 py-0"
        >
          {sessionMode === "agent" ? $t("chat.mode_agent") : $t("chat.mode_libre")}
        </Badge>
        {#if sessionStatus === "closed"}
          <Badge variant="outline" class="text-[10px] px-1.5 py-0">
            {$t("chat.status_closed")}
          </Badge>
        {/if}
      </div>
    </div>
    <div class="flex items-center gap-1">
      <button
        onclick={() => (configOpen = true)}
        class="inline-flex h-8 w-8 items-center justify-center rounded-lg text-muted-foreground transition-all hover:glass-inset hover:text-foreground hover:scale-105 active:scale-95"
        title={$t("chat.config_title")}
        data-testid="chat-config-button"
      >
        <Settings2 class="h-4 w-4" />
      </button>
      <button
        onclick={onclose}
        class="inline-flex h-8 w-8 items-center justify-center rounded-lg text-muted-foreground transition-all hover:glass-inset hover:text-foreground hover:scale-105 active:scale-95"
        data-testid="chat-close-button"
      >
        <X class="h-4 w-4" />
      </button>
    </div>
  </div>

  <!-- Messages area -->
  {#if loading}
    <div class="flex flex-1 items-center justify-center">
      <Loader2 class="h-5 w-5 animate-spin text-muted-foreground" />
    </div>
  {:else if loadError}
    <div class="flex flex-1 items-center justify-center px-4">
      <div class="rounded-xl border border-destructive/30 bg-destructive/5 px-6 py-4">
        <p class="text-sm text-[hsl(var(--destructive))]">{loadError}</p>
      </div>
    </div>
  {:else}
    <div
      bind:this={messagesContainer}
      onscroll={handleScroll}
      class="flex-1 space-y-3 overflow-y-auto px-4 py-4"
    >
      {#if (messages ?? []).length === 0 && !isStreaming && !isProcessing}
        <div class="flex h-full flex-col items-center justify-center gap-3">
          <div class="flex h-12 w-12 items-center justify-center rounded-2xl glass-card">
            <MessageSquare class="h-6 w-6 text-primary/60" />
          </div>
          <p class="text-sm text-muted-foreground">{$t("chat.first_message_placeholder")}</p>
        </div>
      {:else}
        {#each messages ?? [] as message (message.id)}
          <ChatMessageBubble {message} {sessionId} />
        {/each}

        <!-- Streaming indicator (Chat Libre) -->
        {#if isStreaming && sessionMode === "libre"}
          <div class="flex justify-start" data-testid="chat-message-streaming">
            <div class="max-w-[75%] rounded-xl glass-card px-4 py-2.5 text-sm text-foreground">
              <StreamingText text={tokenBuffer} />
            </div>
          </div>
        {/if}

        <!-- Tool execution indicator (visible during ReAct loop tool calls) -->
        {#if activeToolName}
          <div class="flex justify-start" data-testid="chat-tool-executing">
            <div class="flex items-center gap-2 rounded-xl glass-inset px-3 py-1.5 text-xs text-muted-foreground">
              <Loader2 class="h-3 w-3 animate-spin" />
              <span>{$t("chat.tool_executing", { values: { tool: activeToolName } })}</span>
            </div>
          </div>
        {/if}

        <!-- Agent processing indicator -->
        {#if isProcessing && sessionMode === "agent"}
          <div class="flex justify-start" data-testid="chat-agent-loading">
            <div class="flex items-center gap-2 rounded-xl glass-card px-4 py-2.5 text-sm text-muted-foreground">
              <Loader2 class="h-3.5 w-3.5 animate-spin" />
              <span>{$t("chat.agent_processing")}</span>
            </div>
          </div>
        {/if}

        <!-- Generic processing indicator (Chat Libre, before tokens arrive) -->
        {#if isProcessing && sessionMode === "libre"}
          <div class="flex justify-start">
            <div class="flex items-center gap-2 rounded-xl glass-card px-4 py-2.5 text-sm text-muted-foreground">
              <Loader2 class="h-3.5 w-3.5 animate-spin" />
              <span>{$t("chat.thinking")}</span>
            </div>
          </div>
        {/if}
      {/if}
    </div>
  {/if}

  <!-- Input -->
  <ChatInput disabled={inputDisabled} onsend={handleSend} />
</div>

<!-- Config side panel -->
<ChatConfigPanel
  open={configOpen}
  session={sessionDetail}
  onclose={() => (configOpen = false)}
  onupdated={() => void refreshSession()}
/>
