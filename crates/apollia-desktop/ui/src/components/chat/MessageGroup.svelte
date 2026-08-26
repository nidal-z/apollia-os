<script lang="ts">
  import { t, locale } from "svelte-i18n";
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
    /** True while a turn is generating - disables regenerate/edit in bubbles. */
    busy?: boolean;
    /** Regenerate the reply to an assistant turn (truncate-in-place). */
    onregenerate?: (messageId: string) => void;
    /** Replace a user message and re-run from it (truncate-in-place). */
    onedit?: (messageId: string, content: string) => void;
  }

  let {
    group,
    sessionId,
    agentName = null,
    variant = "default",
    busy = false,
    onregenerate,
    onedit,
  }: Props = $props();

  const isUser = $derived(group.role === "user");

  // Only the assistant turn carries the avatar + name + time header (the .who
  // row in the mockup). User turns render as a bare right-aligned bubble, and
  // system / tool messages stay standalone with no header.
  const showHeader = $derived(group.role === "assistant");

  const displayName = $derived.by(() => {
    if (isUser) return $t("chat.you");
    return agentName ?? $t("chat.assistant");
  });

  const headerTime = $derived.by(() => {
    const date = new Date(group.startedAt);
    return date.toLocaleTimeString($locale ?? "en", { hour: "2-digit", minute: "2-digit" });
  });
</script>

<div
  class="flex flex-col items-start"
  data-testid="chat-message-group-{group.key}"
>
  {#if showHeader}
    <div class="flex items-center gap-2 mb-3 px-0.5">
      <Avatar name={displayName} size="xs" ring={false} />
      <span class="text-body-sm font-semibold text-foreground">{displayName}</span>
      <span class="text-caption-lg tabular-nums text-muted-foreground/50">{headerTime}</span>
    </div>
  {/if}

  <div class="w-full flex flex-col items-start space-y-2">
    {#each group.messages as msg (msg.id)}
      <ChatMessageBubble
        message={msg}
        {sessionId}
        showTimestamp={false}
        {variant}
        {busy}
        {onregenerate}
        {onedit}
      />
    {/each}
  </div>
</div>
