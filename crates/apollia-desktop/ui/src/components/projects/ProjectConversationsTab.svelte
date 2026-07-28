<script lang="ts">
  /**
   * ProjectConversationsTab - chats scoped to the selected project.
   *
   * Presentational: the parent owns loading and IPC, this renders the list,
   * the loading placeholder, and the empty state.
   */
  import { t } from "svelte-i18n";
  import { FolderOpen } from "lucide-svelte";
  import {
    SectionTitle,
    EmptyState,
    ConversationRow,
  } from "$lib/components/operator";
  import { SkeletonList } from "$lib/components/operator";
  import type { ChatSessionSummary } from "$lib/types";

  interface Props {
    chats: ChatSessionSummary[];
    loading: boolean;
    formatRelative: (iso: string | null | undefined) => string;
    onOpen: (sessionId: string) => void;
  }

  let { chats, loading, formatRelative, onOpen }: Props = $props();
</script>

<SectionTitle count={chats.length}>
  {$t("projects.tab_conversations")}
</SectionTitle>
<div class="px-8 pb-6">
  {#if loading}
    <SkeletonList count={3} avatar rowClass="px-4 py-2" />
  {:else if chats.length === 0}
    <EmptyState
      title={$t("projects.no_conversations")}
      desc={$t("projects.start_conversation_hint")}
    >
      {#snippet icon()}<FolderOpen size={20} />{/snippet}
    </EmptyState>
  {:else}
    <div
      class="border border-border rounded-xl overflow-hidden bg-card"
      data-testid="project-conversations-list"
    >
      {#each chats as chat (chat.id)}
        <ConversationRow
          title={chat.title ??
            chat.agent_name ??
            $t("chat.untitled_session")}
          lastMessage={chat.last_message_preview ?? undefined}
          timestamp={formatRelative(chat.created_at)}
          state={chat.status === "closed" ? "closed" : "default"}
          live={chat.status === "processing"}
          onclick={() => onOpen(chat.id)}
        />
      {/each}
    </div>
  {/if}
</div>
