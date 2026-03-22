<script lang="ts">
  import { t } from "svelte-i18n";
  import { Bot, MessageSquare, Trash2 } from "lucide-svelte";
  import type { ChatSessionSummary } from "$lib/types";

  interface Props {
    session: ChatSessionSummary;
    selected?: boolean;
    onclick: (id: string) => void;
    ondelete?: (id: string) => void;
  }

  let { session, selected = false, onclick, ondelete }: Props = $props();

  const isClosed = $derived(session.status === "closed");

  const relativeTime = $derived.by(() => {
    const now = Date.now();
    const created = new Date(session.created_at).getTime();
    const diffMs = now - created;
    const diffMinutes = Math.floor(diffMs / 60_000);
    if (diffMinutes < 1) return $t("chat.just_now");
    if (diffMinutes < 60) return $t("chat.minutes_ago", { values: { n: diffMinutes } });
    const diffHours = Math.floor(diffMinutes / 60);
    if (diffHours < 24) return $t("chat.hours_ago", { values: { n: diffHours } });
    const diffDays = Math.floor(diffHours / 24);
    return $t("chat.days_ago", { values: { n: diffDays } });
  });

  const title = $derived.by(() => {
    if (session.mode === "agent" && session.agent_name) return session.agent_name;
    return $t("chat.mode_libre");
  });

  const preview = $derived.by(() => {
    if (session.last_message_preview) {
      return session.last_message_preview.length > 60
        ? session.last_message_preview.slice(0, 60) + "\u2026"
        : session.last_message_preview;
    }
    const count = session.message_count ?? 0;
    if (count > 0) return `${count} ${$t("chat.messages_suffix")}`;
    return null;
  });

  function handleDelete(event: MouseEvent) {
    event.stopPropagation();
    ondelete?.(session.id);
  }
</script>

<button
  class="group relative w-full rounded-lg px-3 py-2.5 text-left transition-all duration-150
    {selected
      ? 'bg-primary/8 text-foreground'
      : isClosed
        ? 'opacity-50 hover:opacity-70 hover:bg-muted/30'
        : 'hover:bg-muted/40'}"
  data-testid="chat-session-card-{session.id}"
  onclick={() => onclick(session.id)}
>
  {#if ondelete}
    <button
      class="absolute right-2 top-2 rounded-md p-1 text-muted-foreground opacity-0 group-hover:opacity-100
        hover:bg-destructive/10 hover:text-destructive transition-all"
      onclick={handleDelete}
      title={$t("chat.delete_session")}
      data-testid="chat-session-delete-{session.id}"
    >
      <Trash2 size={12} />
    </button>
  {/if}

  <div class="flex items-center gap-2.5">
    {#if session.mode === "agent"}
      <Bot size={14} class="shrink-0 text-primary" />
    {:else}
      <MessageSquare size={14} class="shrink-0 text-muted-foreground" />
    {/if}
    <span class="flex-1 min-w-0 text-[13px] font-medium truncate">{title}</span>
    <span class="text-[10px] text-muted-foreground/40 shrink-0">{relativeTime}</span>
  </div>

  {#if preview}
    <p class="mt-1 text-[11px] text-muted-foreground/60 truncate pl-[22px]">{preview}</p>
  {/if}
</button>
