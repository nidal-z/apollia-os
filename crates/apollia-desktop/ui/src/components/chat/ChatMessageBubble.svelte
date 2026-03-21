<script lang="ts">
  import { fly } from "svelte/transition";
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
</script>

<div
  class="flex {isUser ? 'justify-end' : 'justify-start'}"
  data-testid="chat-message-{message.id}"
  in:fly={{ y: 4, duration: 200 }}
>
  <div
    class="max-w-[75%] rounded-xl px-4 py-2.5 text-sm {isUser
      ? 'bg-primary text-primary-foreground shadow-sm'
      : 'glass-card border border-border text-foreground'}"
  >
    {#if message.content}
      <p class="whitespace-pre-wrap break-words">{message.content}</p>
    {/if}

    {#if hasToolCalls}
      <div class="mt-1">
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

    <p class="mt-1 text-[11px] {isUser ? 'text-primary-foreground/50 text-right' : 'text-muted-foreground/50 text-left'}">
      {formattedTime}
    </p>
  </div>
</div>
