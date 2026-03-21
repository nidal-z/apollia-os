<script lang="ts">
  import { t } from "svelte-i18n";
  import { Bot, MessageSquare } from "lucide-svelte";
  import type { ChatSessionSummary } from "$lib/types";
  import { Badge } from "$lib/components/ui/badge";

  interface Props {
    session: ChatSessionSummary;
    onclick: (id: string) => void;
  }

  let { session, onclick }: Props = $props();

  const isClosed = $derived(session.status === "closed");

  const modeBadgeVariant = $derived(
    session.mode === "agent" ? "default" as const : "secondary" as const,
  );

  const statusBadgeVariant = $derived(
    isClosed ? "outline" as const : "success" as const,
  );

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

  const subtitle = $derived.by(() => {
    if (session.mode === "agent" && session.agent_name) {
      return session.agent_name;
    }
    return $t("chat.mode_libre");
  });

  const preview = $derived(
    session.last_message_preview
      ? session.last_message_preview.length > 80
        ? session.last_message_preview.slice(0, 80) + "\u2026"
        : session.last_message_preview
      : $t("chat.no_messages"),
  );
</script>

<button
  class="group w-full rounded-xl glass-card glass-border border p-4 text-left
    transition-all hover:shadow-lg hover:scale-[1.01] active:scale-[0.99]
    {isClosed ? 'opacity-55' : ''}"
  data-testid="chat-session-card-{session.id}"
  onclick={() => onclick(session.id)}
>
  <div class="flex items-start justify-between gap-2">
    <div class="flex items-center gap-2.5">
      <div class="flex h-7 w-7 items-center justify-center rounded-lg glass-inset">
        {#if session.mode === "agent"}
          <Bot class="h-3.5 w-3.5 text-primary" />
        {:else}
          <MessageSquare class="h-3.5 w-3.5 text-secondary" />
        {/if}
      </div>
      <div class="flex items-center gap-2">
        <Badge variant={modeBadgeVariant} class="text-[10px] px-1.5 py-0">
          {session.mode === "agent" ? $t("chat.mode_agent") : $t("chat.mode_libre")}
        </Badge>
        <span class="text-sm font-medium">{subtitle}</span>
      </div>
    </div>
    <Badge variant={statusBadgeVariant} class="text-[10px] px-1.5 py-0">
      {isClosed ? $t("chat.status_closed") : $t("chat.status_active")}
    </Badge>
  </div>

  <p class="mt-2.5 text-xs text-muted-foreground line-clamp-2">{preview}</p>

  <div class="mt-2.5 flex items-center justify-between text-[11px] text-muted-foreground/60">
    <span>{$t("chat.message_count", { values: { n: session.message_count ?? 0 } })}</span>
    <span>{relativeTime}</span>
  </div>
</button>
