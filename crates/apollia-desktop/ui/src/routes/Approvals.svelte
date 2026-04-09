<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { fly } from "svelte/transition";
  import { flip } from "svelte/animate";
  import { t } from "svelte-i18n";
  import type { PendingApproval, ResolvedApproval } from "$lib/types";
  import { pendingApprovals, pendingCount, requestNotificationPermission } from "$lib/stores/hitl";
  import { pendingChatApprovals, pendingChatApprovalCount } from "$lib/stores/chat";
  import { pendingChatSessionId } from "$lib/stores/chat";
  import { navigateTo } from "$lib/stores/navigation";
  import { Badge } from "$lib/components/ui/badge";
  import { Separator } from "$lib/components/ui/separator";
  import { Skeleton } from "$lib/components/ui/skeleton";
  import { ShieldCheck, MessageSquare } from "lucide-svelte";
  import ApprovalCard from "../components/hitl/ApprovalCard.svelte";
  import ChatApprovalCard from "../components/chat/ApprovalCard.svelte";
  import ApprovalHistory from "../components/hitl/ApprovalHistory.svelte";
  import EmptyState from "../components/common/EmptyState.svelte";

  let loading = $state(true);
  let error = $state<string | null>(null);
  let history = $state<ResolvedApproval[]>([]);

  /** Total pending count (HITL + chat). */
  const totalPendingCount = $derived($pendingCount + $pendingChatApprovalCount);

  onMount(() => {
    requestNotificationPermission();
    loadData();
  });

  async function loadData(): Promise<void> {
    loading = true;
    error = null;
    try {
      const [pending, resolved] = await Promise.all([
        invoke<PendingApproval[]>("list_pending_approvals"),
        invoke<ResolvedApproval[]>("list_resolved_approvals", { limit: 20, days: 7 }),
      ]);
      pendingApprovals.set(pending);
      history = resolved;
    } catch (err: unknown) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      loading = false;
    }
  }

  function openChatSession(sessionId: string): void {
    pendingChatSessionId.set(sessionId);
    navigateTo("chat");
  }
</script>

<div class="max-w-6xl space-y-6" data-testid="approvals-page">
  <!-- Header -->
  <div class="flex items-center justify-between">
    <div>
      <div class="flex items-center gap-3">
        <h1 class="text-2xl font-semibold" data-testid="approvals-header">{$t('approvals.title')}</h1>
        {#if totalPendingCount > 0}
          <Badge variant="destructive" data-testid="approvals-pending-count">{$t('approvals.pending_count', { values: { count: totalPendingCount } })}</Badge>
        {/if}
      </div>
      <p class="mt-1 text-xs text-muted-foreground" data-testid="approvals-subtitle">{$t('approvals.subtitle')}</p>
    </div>
  </div>

  {#if loading}
    <div class="space-y-3">
      <Skeleton width="100%" height="4rem" />
      <Skeleton width="100%" height="4rem" />
      <Skeleton width="60%" height="1rem" />
    </div>
  {:else if error}
    <p class="text-sm text-destructive">{error}</p>
  {:else}
    <!-- Pending approvals section -->
    <section>
      <h2 class="mb-3 text-sm font-medium uppercase tracking-wider text-muted-foreground" data-testid="approvals-pending-title">{$t('approvals.pending_title')}</h2>
      {#if $pendingApprovals.length === 0}
        <EmptyState
          icon={ShieldCheck}
          title={$t('approvals.no_pending')}
          subtitle={$t('approvals.empty_subtitle')}
          page="approvals"
        />
      {:else}
        <div class="space-y-3">
          {#each $pendingApprovals as approval (approval.task_id)}
            <div animate:flip={{ duration: 300 }} in:fly={{ y: 10, duration: 200 }}>
              <ApprovalCard {approval} />
            </div>
          {/each}
        </div>
      {/if}
    </section>

    <!-- Chat tool approvals section -->
    {#if $pendingChatApprovals.length > 0}
      <Separator />
      <section>
        <h2 class="mb-3 text-sm font-medium uppercase tracking-wider text-muted-foreground flex items-center gap-2" data-testid="approvals-chat-title">
          <MessageSquare size={13} />
          {$t('approvals.chat_tools_title')}
        </h2>
        <div class="space-y-3">
          {#each $pendingChatApprovals as chatApproval (`${chatApproval.sessionId}::${chatApproval.messageId}::${chatApproval.toolName}`)}
            <div animate:flip={{ duration: 300 }} in:fly={{ y: 10, duration: 200 }}>
              <div class="glass-card-hover relative overflow-hidden" data-testid="chat-approval-wrapper">
                <div class="h-0.5 w-full bg-warning"></div>
                <div class="px-3.5 pt-2 pb-1 flex items-center justify-between">
                  <span class="text-[10px] text-muted-foreground">
                    {$t('approvals.chat_session_label')}: {chatApproval.sessionId.slice(0, 8)}
                  </span>
                  <button
                    class="text-[10px] text-primary hover:underline"
                    onclick={() => openChatSession(chatApproval.sessionId)}
                    data-testid="chat-approval-open-session"
                  >
                    {$t('approvals.chat_open_session')} →
                  </button>
                </div>
                <div class="px-2 pb-2">
                  <ChatApprovalCard
                    sessionId={chatApproval.sessionId}
                    messageId={chatApproval.messageId}
                    toolName={chatApproval.toolName}
                    inputPreview={chatApproval.inputPreview}
                  />
                </div>
              </div>
            </div>
          {/each}
        </div>
      </section>
    {/if}

    <Separator />

    <!-- History section -->
    <section>
      <h2 class="mb-3 text-sm font-medium uppercase tracking-wider text-muted-foreground">{$t('approvals.history_title')}</h2>
      <ApprovalHistory {history} />
    </section>
  {/if}
</div>
