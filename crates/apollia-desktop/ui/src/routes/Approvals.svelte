<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { fly } from "svelte/transition";
  import { flip } from "svelte/animate";
  import { t } from "svelte-i18n";
  import type { PendingApproval, ResolvedApproval, ResolvedChatApproval } from "$lib/types";
  import { pendingApprovals, pendingCount, requestNotificationPermission } from "$lib/stores/hitl";
  import { pendingChatApprovals, pendingChatApprovalCount } from "$lib/stores/chat";
  import { pendingChatSessionId } from "$lib/stores/chat";
  import { navigateTo } from "$lib/stores/navigation";
  import { Badge } from "$lib/components/ui/badge";
  import { Separator } from "$lib/components/ui/separator";
  import { Skeleton } from "$lib/components/ui/skeleton";
  import { ShieldCheck, Bot, MessageSquare, ClipboardList } from "lucide-svelte";
  import ApprovalCard from "../components/hitl/ApprovalCard.svelte";
  import ChatApprovalCard from "../components/chat/ApprovalCard.svelte";
  import EmptyState from "../components/common/EmptyState.svelte";

  /** Unified history entry — covers both HITL and chat approvals. */
  interface UnifiedHistoryEntry {
    type: "task" | "chat";
    id: string;
    label: string;
    approved: boolean;
    detail: string;
    waitDuration: number | null;
    respondedAt: string | null;
  }

  let loading = $state(true);
  let error = $state<string | null>(null);
  let unifiedHistory = $state<UnifiedHistoryEntry[]>([]);

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
      const [pending, resolved, chatResolved] = await Promise.all([
        invoke<PendingApproval[]>("list_pending_approvals"),
        invoke<ResolvedApproval[]>("list_resolved_approvals", { limit: 20, days: 7 }),
        invoke<ResolvedChatApproval[]>("list_chat_approval_history", { limit: 20, days: 7 }).catch(() => [] as ResolvedChatApproval[]),
      ]);
      pendingApprovals.set(pending);

      // Build unified history
      const taskEntries: UnifiedHistoryEntry[] = resolved.map((r) => ({
        type: "task" as const,
        id: r.task_id.slice(0, 8),
        label: r.agent_name || "-",
        approved: r.approved,
        detail: r.reason ?? "-",
        waitDuration: r.wait_duration_ms,
        respondedAt: r.responded_at,
      }));

      const chatEntries: UnifiedHistoryEntry[] = chatResolved.map((r) => ({
        type: "chat" as const,
        id: r.session_id.slice(0, 8),
        label: r.tool_name,
        approved: r.decision !== "refuse",
        detail: decisionLabel(r.decision),
        waitDuration: null,
        respondedAt: r.resolved_at,
      }));

      // Merge and sort by date descending
      unifiedHistory = [...taskEntries, ...chatEntries].sort((a, b) => {
        const dateA = a.respondedAt ? new Date(a.respondedAt).getTime() : 0;
        const dateB = b.respondedAt ? new Date(b.respondedAt).getTime() : 0;
        return dateB - dateA;
      });
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

  function decisionLabel(decision: string): string {
    switch (decision) {
      case "accept": return $t("common.approved");
      case "always_accept": return $t("approvals.always_approved");
      case "refuse": return $t("common.rejected");
      default: return decision;
    }
  }

  function formatWaitDuration(ms: number | null): string {
    if (ms === null || ms === undefined) return "-";
    if (ms < 1_000) return `${ms}ms`;
    const totalSecs = Math.floor(ms / 1_000);
    if (totalSecs < 60) return `${totalSecs}s`;
    const mins = Math.floor(totalSecs / 60);
    const secs = totalSecs % 60;
    if (mins < 60) return `${mins}m ${secs}s`;
    const hours = Math.floor(mins / 60);
    const remMins = mins % 60;
    return `${hours}h ${remMins}m`;
  }

  function formatDate(iso: string | null): string {
    if (!iso) return "-";
    return new Date(iso).toLocaleString();
  }
</script>

<div class="mx-auto w-full max-w-6xl space-y-6" data-testid="approvals-page">
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
    <!-- Unified pending section -->
    <section>
      <h2 class="mb-3 text-sm font-medium uppercase tracking-wider text-muted-foreground" data-testid="approvals-pending-title">{$t('approvals.pending_title')}</h2>

      {#if totalPendingCount === 0}
        <EmptyState
          icon={ShieldCheck}
          title={$t('approvals.no_pending')}
          subtitle={$t('approvals.empty_subtitle')}
          page="approvals"
        />
      {:else}
        <div class="space-y-3">
          <!-- HITL task approvals -->
          {#each $pendingApprovals as approval (approval.task_id)}
            <div animate:flip={{ duration: 300 }} in:fly={{ y: 10, duration: 200 }}>
              <div class="relative">
                <div class="absolute -left-0.5 top-2.5 z-10 flex items-center gap-1 rounded-r-md bg-primary/10 px-2 py-0.5">
                  <Bot size={10} class="text-primary" />
                  <span class="text-[9px] font-medium uppercase tracking-wide text-primary">{$t('approvals.type_task')}</span>
                </div>
                <ApprovalCard {approval} />
              </div>
            </div>
          {/each}

          <!-- Chat tool approvals -->
          {#each $pendingChatApprovals as chatApproval (`${chatApproval.sessionId}::${chatApproval.messageId}::${chatApproval.toolName}`)}
            <div animate:flip={{ duration: 300 }} in:fly={{ y: 10, duration: 200 }}>
              <div class="glass-card-hover relative overflow-hidden" data-testid="chat-approval-wrapper">
                <div class="h-0.5 w-full bg-warning"></div>
                <div class="px-3.5 pt-2 pb-1 flex items-center justify-between">
                  <div class="flex items-center gap-2">
                    <div class="flex items-center gap-1 rounded-md bg-secondary/10 px-1.5 py-0.5">
                      <MessageSquare size={10} class="text-secondary" />
                      <span class="text-[9px] font-medium uppercase tracking-wide text-secondary">{$t('approvals.type_chat')}</span>
                    </div>
                    <span class="text-[10px] text-muted-foreground">
                      {$t('approvals.chat_session_label')}: {chatApproval.sessionId.slice(0, 8)}
                    </span>
                  </div>
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
      {/if}
    </section>

    <Separator />

    <!-- Unified history section -->
    <section>
      <h2 class="mb-3 text-sm font-medium uppercase tracking-wider text-muted-foreground">{$t('approvals.history_title')}</h2>

      {#if unifiedHistory.length === 0}
        <div
          class="flex flex-col items-center justify-center py-12 text-muted-foreground"
          data-testid="approval-history-empty"
        >
          <ClipboardList class="h-8 w-8 mb-2 opacity-50" />
          <p class="text-sm">{$t("approvals.no_history")}</p>
        </div>
      {:else}
        <div class="glass-card glass-border rounded-lg overflow-x-auto" data-testid="approval-history-table">
          <table class="w-full min-w-[600px] text-[13px]">
            <thead class="border-b border-border bg-muted/50">
              <tr>
                <th class="text-left px-3 py-2 text-[11px] text-muted-foreground font-medium">{$t("approvals.table.type")}</th>
                <th class="text-left px-3 py-2 text-[11px] text-muted-foreground font-medium">{$t("approvals.table.id")}</th>
                <th class="text-left px-3 py-2 text-[11px] text-muted-foreground font-medium">{$t("approvals.table.label")}</th>
                <th class="text-left px-3 py-2 text-[11px] text-muted-foreground font-medium">{$t("approvals.table.result")}</th>
                <th class="text-left px-3 py-2 text-[11px] text-muted-foreground font-medium">{$t("approvals.table.wait")}</th>
                <th class="text-left px-3 py-2 text-[11px] text-muted-foreground font-medium">{$t("approvals.table.detail")}</th>
                <th class="text-left px-3 py-2 text-[11px] text-muted-foreground font-medium">{$t("approvals.table.date")}</th>
              </tr>
            </thead>
            <tbody>
              {#each unifiedHistory as item (`${item.type}-${item.id}-${item.respondedAt}`)}
                <tr class="hover:bg-muted border-b border-border last:border-0">
                  <td class="px-3 py-2">
                    {#if item.type === "task"}
                      <div class="flex items-center gap-1">
                        <Bot size={12} class="text-primary" />
                        <span class="text-[10px] text-primary font-medium">{$t('approvals.type_task')}</span>
                      </div>
                    {:else}
                      <div class="flex items-center gap-1">
                        <MessageSquare size={12} class="text-secondary" />
                        <span class="text-[10px] text-secondary font-medium">{$t('approvals.type_chat')}</span>
                      </div>
                    {/if}
                  </td>
                  <td class="px-3 py-2">
                    <code class="text-[11px]">{item.id}</code>
                  </td>
                  <td class="px-3 py-2 text-[11px]">{item.label}</td>
                  <td class="px-3 py-2">
                    {#if item.approved}
                      <Badge variant="success" data-testid="approval-history-badge">
                        {$t("common.approved")}
                      </Badge>
                    {:else}
                      <Badge variant="destructive" data-testid="approval-history-badge">
                        {$t("common.rejected")}
                      </Badge>
                    {/if}
                  </td>
                  <td class="px-3 py-2 text-[11px] text-muted-foreground">
                    {formatWaitDuration(item.waitDuration)}
                  </td>
                  <td class="max-w-[200px] truncate px-3 py-2 text-[11px] text-muted-foreground" title={item.detail}>
                    {item.detail}
                  </td>
                  <td class="px-3 py-2 text-[11px] text-muted-foreground">
                    {formatDate(item.respondedAt)}
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {/if}
    </section>
  {/if}
</div>
