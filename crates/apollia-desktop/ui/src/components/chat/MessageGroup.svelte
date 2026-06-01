<script lang="ts">
  import { t } from "svelte-i18n";
  import { Avatar } from "$lib/components/ui/avatar";
  import type { MessageGroup } from "$lib/chat/groupMessages";
  import ChatMessageBubble from "./ChatMessageBubble.svelte";

  interface Props {
    group: MessageGroup;
    sessionId: string;
    /** Display name for the assistant side - falls back to a generic label. */
    agentName?: string | null;
    /** "compact" propagates to bubbles for embedded contexts. */
    variant?: "default" | "compact";
  }

  let {
    group,
    sessionId,
    agentName = null,
    variant = "default",
  }: Props = $props();

  const isUser = $derived(group.role === "user");

  // System / tool messages stay standalone - no header, plain bubbles.
  const showHeader = $derived(group.role === "user" || group.role === "assistant");

  const displayName = $derived.by(() => {
    if (isUser) return $t("chat.you", { default: "You" });
    return agentName ?? $t("chat.assistant", { default: "Assistant" });
  });

  const headerTime = $derived.by(() => {
    const date = new Date(group.startedAt);
    return date.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  });
</script>

<div
  class="flex flex-col {isUser ? 'items-end' : 'items-start'}"
  data-testid="chat-message-group-{group.key}"
>
  {#if showHeader}
    <div
      class="flex items-center gap-2 mb-1 px-1 {isUser ? 'flex-row-reverse' : 'flex-row'}"
    >
      <Avatar name={displayName} size="xs" ring={false} />
      <span class="text-[11px] font-medium text-muted-foreground/70">{displayName}</span>
      <span class="text-[10px] text-muted-foreground/40">{headerTime}</span>
    </div>
  {/if}

  <div class="w-full flex flex-col space-y-1 {isUser ? 'items-end' : 'items-start'}">
    {#each group.messages as msg, i (msg.id)}
      <ChatMessageBubble
        message={msg}
        {sessionId}
        showTimestamp={i === group.messages.length - 1}
        {variant}
      />
    {/each}
  </div>
</div>
