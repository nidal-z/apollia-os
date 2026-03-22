<script lang="ts">
  import { fly } from "svelte/transition";
  import { t } from "svelte-i18n";
  import { Copy, Check } from "lucide-svelte";
  import type { ChatMessageView } from "$lib/types";
  import ToolCallCard from "./ToolCallCard.svelte";
  import ApprovalCard from "./ApprovalCard.svelte";

  interface Props {
    message: ChatMessageView;
    sessionId: string;
  }

  let { message, sessionId }: Props = $props();

  const isUser = $derived(message.role === "user");

  const formattedTime = $derived.by(() => {
    const date = new Date(message.created_at);
    return date.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  });

  const hasToolCalls = $derived(
    message.tool_calls !== null && message.tool_calls.length > 0,
  );

  let copied = $state(false);

  async function handleCopy(): Promise<void> {
    if (!message.content) return;
    try {
      await navigator.clipboard.writeText(message.content);
      copied = true;
      setTimeout(() => { copied = false; }, 1500);
    } catch {
      // clipboard API may not be available
    }
  }
</script>

<div
  class="group flex {isUser ? 'justify-end' : 'justify-start'}"
  data-testid="chat-message-{message.id}"
  in:fly={{ y: 4, duration: 200 }}
>
  <div
    class="relative max-w-[72%] px-3.5 py-2.5 text-[13px] leading-relaxed {isUser
      ? 'bg-primary text-primary-foreground rounded-2xl rounded-br-sm'
      : 'bg-card/80 border border-border/30 rounded-2xl rounded-bl-sm text-foreground'}"
  >
    <!-- Copy button (assistant only) -->
    {#if message.content && !isUser}
      <button
        onclick={handleCopy}
        class="absolute -top-1.5 -right-1.5 h-5 w-5 rounded flex items-center justify-center
          bg-card border border-border/40 text-muted-foreground/50
          opacity-0 group-hover:opacity-100
          hover:text-foreground transition-all shadow-sm"
        title={$t("chat.copy_message")}
      >
        {#if copied}
          <Check size={10} class="text-success" />
        {:else}
          <Copy size={10} />
        {/if}
      </button>
    {/if}

    {#if message.content}
      <p class="whitespace-pre-wrap break-words">{message.content}</p>
    {/if}

    {#if hasToolCalls}
      <div class="mt-2 space-y-1.5">
        {#each message.tool_calls ?? [] as toolCall (toolCall.tool_name)}
          {#if toolCall.status === "pending"}
            <ApprovalCard
              {sessionId}
              messageId={message.id}
              toolName={toolCall.tool_name}
              inputPreview={JSON.stringify(toolCall.input, null, 2)}
            />
          {:else}
            <ToolCallCard {toolCall} />
          {/if}
        {/each}
      </div>
    {/if}

    <p class="mt-1 text-[10px] {isUser ? 'text-primary-foreground/40 text-right' : 'text-muted-foreground/40 text-left'}">
      {formattedTime}
    </p>
  </div>
</div>
