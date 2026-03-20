<script lang="ts">
  import { onMount, onDestroy, tick } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { t } from "svelte-i18n";
  import { X, Loader2 } from "lucide-svelte";
  import { currentSession, chatTokenBuffer } from "$lib/stores/chat";
  import type { ChatSessionDetail, ChatMessageView } from "$lib/types";
  import ChatMessageBubble from "./ChatMessageBubble.svelte";
  import ChatInput from "./ChatInput.svelte";
  import StreamingText from "./StreamingText.svelte";

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

  const headerTitle = $derived(
    sessionMode === "agent" && sessionAgentName
      ? sessionAgentName
      : $t("chat.mode_libre"),
  );

  const inputDisabled = $derived(
    isProcessing || isStreaming || sessionStatus === "closed",
  );

  let unlistenToken: UnlistenFn | undefined;
  let unlistenChanged: UnlistenFn | undefined;

  onMount(async () => {
    await loadSession();

    unlistenToken = await listen<{ session_id: string; message_id: string; token: string }>(
      "chat-token",
      (event) => {
        if (event.payload.session_id !== sessionId) return;
        tokenBuffer += event.payload.token;
        isStreaming = true;
        isProcessing = false;
        scrollToBottom();
      },
    );

    unlistenChanged = await listen<{ category: string }>(
      "runtime-event",
      (event) => {
        if (event.payload.category === "chat-changed") {
          void refreshSession();
        }
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
      if (isStreaming) {
        isStreaming = false;
        tokenBuffer = "";
        chatTokenBuffer.set("");
      }
      scrollToBottom();
    } catch {
      // Session may have been deleted — close view
    }
  }

  function applySessionDetail(detail: ChatSessionDetail): void {
    messages = detail.messages;
    sessionMode = detail.mode;
    sessionAgentName = detail.agent_name;
    sessionStatus = detail.status;
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
    // Optimistic UI: add user message immediately
    const tempMsg: ChatMessageView = {
      id: `temp-${Date.now()}`,
      role: "user",
      content,
      tool_calls: null,
      tool_name: null,
      seq: messages.length,
      created_at: new Date().toISOString(),
    };
    messages = [...messages, tempMsg];
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
        seq: messages.length,
        created_at: new Date().toISOString(),
      };
      messages = [...messages, errMsg];
      scrollToBottom();
    }
  }
</script>

<div class="flex h-full flex-col" data-testid="chat-conversation">
  <!-- Header -->
  <div class="flex items-center justify-between border-b px-4 py-3">
    <h2 class="text-sm font-semibold">{headerTitle}</h2>
    <button
      onclick={onclose}
      class="inline-flex h-8 w-8 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-primary/5 hover:text-foreground"
      data-testid="chat-close-button"
    >
      <X class="h-4 w-4" />
    </button>
  </div>

  <!-- Messages area -->
  {#if loading}
    <div class="flex flex-1 items-center justify-center">
      <Loader2 class="h-5 w-5 animate-spin text-muted-foreground" />
    </div>
  {:else if loadError}
    <div class="flex flex-1 items-center justify-center px-4">
      <p class="text-sm text-[hsl(var(--destructive))]">{loadError}</p>
    </div>
  {:else}
    <div
      bind:this={messagesContainer}
      onscroll={handleScroll}
      class="flex-1 space-y-3 overflow-y-auto px-4 py-4"
    >
      {#if messages.length === 0 && !isStreaming && !isProcessing}
        <div class="flex h-full items-center justify-center">
          <p class="text-sm text-muted-foreground">{$t("chat.first_message_placeholder")}</p>
        </div>
      {:else}
        {#each messages as message (message.id)}
          <ChatMessageBubble {message} />
        {/each}

        <!-- Streaming indicator (Chat Libre) -->
        {#if isStreaming && sessionMode === "libre"}
          <div class="flex justify-start" data-testid="chat-message-streaming">
            <div class="max-w-[75%] rounded-lg border bg-background px-4 py-2.5 text-sm text-foreground">
              <StreamingText text={tokenBuffer} />
            </div>
          </div>
        {/if}

        <!-- Agent processing indicator (Chat Agent) -->
        {#if isProcessing && sessionMode === "agent"}
          <div class="flex justify-start" data-testid="chat-agent-loading">
            <div class="flex items-center gap-2 rounded-lg border bg-background px-4 py-2.5 text-sm text-muted-foreground">
              <Loader2 class="h-3.5 w-3.5 animate-spin" />
              <span>{$t("chat.agent_processing")}</span>
            </div>
          </div>
        {/if}

        <!-- Generic processing indicator (Chat Libre, before tokens arrive) -->
        {#if isProcessing && sessionMode === "libre"}
          <div class="flex justify-start">
            <div class="flex items-center gap-2 rounded-lg border bg-background px-4 py-2.5 text-sm text-muted-foreground">
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
