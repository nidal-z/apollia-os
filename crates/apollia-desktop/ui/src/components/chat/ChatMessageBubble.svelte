<script lang="ts">
  import type { ChatMessageView } from "$lib/types";

  interface Props {
    message: ChatMessageView;
  }

  let { message }: Props = $props();

  const isUser = $derived(message.role === "user");

  const formattedTime = $derived.by(() => {
    const date = new Date(message.created_at);
    return date.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  });
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
    <p class="whitespace-pre-wrap break-words">{message.content}</p>
    <p class="mt-1 text-[10px] text-muted-foreground/60 {isUser ? 'text-right' : 'text-left'}">
      {formattedTime}
    </p>
  </div>
</div>
