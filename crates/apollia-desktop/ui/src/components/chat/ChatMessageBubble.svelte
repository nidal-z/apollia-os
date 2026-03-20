<script lang="ts">
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
>
  <div
    class="max-w-[75%] rounded-lg px-4 py-2.5 text-sm {isUser
      ? 'glass-surface text-foreground'
      : 'bg-background text-foreground border'}"
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

    <p class="mt-1 text-[10px] text-muted-foreground/60 {isUser ? 'text-right' : 'text-left'}">
      {formattedTime}
    </p>
  </div>
</div>
