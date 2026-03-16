<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import type { PendingApproval, ResolvedApproval } from "$lib/types";
  import { pendingApprovals, pendingCount, requestNotificationPermission } from "$lib/stores/hitl";
  import { Badge } from "$lib/components/ui/badge";
  import { Separator } from "$lib/components/ui/separator";
  import { Skeleton } from "$lib/components/ui/skeleton";
  import ApprovalCard from "../components/hitl/ApprovalCard.svelte";
  import ApprovalHistory from "../components/hitl/ApprovalHistory.svelte";

  let loading = $state(true);
  let error = $state<string | null>(null);
  let history = $state<ResolvedApproval[]>([]);

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
</script>

<div class="space-y-6">
  <!-- Header -->
  <div class="flex items-center gap-3">
    <h1 class="text-2xl font-bold" data-testid="approvals-header">{$t('approvals.title')}</h1>
    {#if $pendingCount > 0}
      <Badge variant="destructive" data-testid="approvals-pending-count">{$t('approvals.pending_count', { values: { count: $pendingCount } })}</Badge>
    {/if}
  </div>

  {#if loading}
    <div class="space-y-3">
      <Skeleton width="100%" height="4rem" />
      <Skeleton width="100%" height="4rem" />
      <Skeleton width="60%" height="1rem" />
    </div>
  {:else if error}
    <p class="text-sm text-[hsl(var(--destructive))]">{error}</p>
  {:else}
    <!-- Pending approvals section -->
    <section>
      <h2 class="mb-3 text-lg font-semibold" data-testid="approvals-pending-title">{$t('approvals.pending_title')}</h2>
      {#if $pendingApprovals.length === 0}
        <div class="flex flex-col items-center justify-center gap-2 rounded-lg border border-dashed py-12">
          <p class="text-muted-foreground">{$t('approvals.no_pending')}</p>
        </div>
      {:else}
        <div class="space-y-3">
          {#each $pendingApprovals as approval (approval.task_id)}
            <ApprovalCard {approval} />
          {/each}
        </div>
      {/if}
    </section>

    <Separator />

    <!-- History section -->
    <section>
      <h2 class="mb-3 text-lg font-semibold">{$t('approvals.history_title')}</h2>
      <ApprovalHistory {history} />
    </section>
  {/if}
</div>
