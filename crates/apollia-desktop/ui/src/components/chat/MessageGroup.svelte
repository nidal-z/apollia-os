<script lang="ts" module>
  import type { ChatMessageStyle } from "$lib/stores/chatAppearance";

  /**
   * Whether a group carries the avatar, name and time row. An assistant turn
   * always does. A user turn does only in the "blocks" style, where no bubble
   * tells it from the answer; system and tool messages never do.
   */
  export function groupShowsHeader(role: string, style: ChatMessageStyle): boolean {
    return role === "assistant" || (role === "user" && style === "block");
  }
</script>

<script lang="ts">
  import { t, locale } from "svelte-i18n";
  import { chatMessageStyle } from "$lib/stores/chatAppearance";
  import { Avatar } from "$lib/components/ui/avatar";
  import type { MessageGroup } from "$lib/chat/groupMessages";
  import ChatMessageBubble from "./ChatMessageBubble.svelte";
  import { isEdited } from "$lib/chat/answerActions";

  interface Props {
    group: MessageGroup;
    sessionId: string;
    /** Display name for the assistant side - falls back to a generic label. */
    agentName?: string | null;
    /** Thread density from Settings > Appearance; "compact" propagates to bubbles. */
    variant?: "default" | "compact";
    /** True while a turn is generating - disables regenerate/edit in bubbles. */
    busy?: boolean;
    /** The answer that keeps its generation figures under "last message only". */
    lastAnswerId?: string | null;
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
    lastAnswerId = null,
    onregenerate,
    onedit,
  }: Props = $props();

  const isUser = $derived(group.role === "user");

  const showHeader = $derived(groupShowsHeader(group.role, $chatMessageStyle));

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
    <div class="flex items-center gap-2 {variant === 'compact' ? 'mb-1.5' : 'mb-3'} px-0.5">
      <Avatar name={displayName} size="xs" ring={false} />
      <span class="text-body-sm font-semibold text-foreground">{displayName}</span>
      <span class="text-caption-lg tabular-nums text-muted-foreground/50">{headerTime}</span>
      {#if group.messages.some(isEdited)}
        <span
          class="rounded-full bg-surface-2 px-2 text-caption text-muted-foreground"
          title={$t("chat.edited_hint")}
          data-testid="chat-message-edited-{group.key}"
        >
          {$t("chat.edited_mark")}
        </span>
      {/if}
    </div>
  {/if}

  <div class="w-full flex flex-col items-start {variant === 'compact' ? 'space-y-1' : 'space-y-2'}">
    {#each group.messages as msg (msg.id)}
      <ChatMessageBubble
        message={msg}
        {sessionId}
        showTimestamp={false}
        {variant}
        {busy}
        {lastAnswerId}
        {onregenerate}
        {onedit}
      />
    {/each}
  </div>
</div>
