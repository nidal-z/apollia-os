<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import type { PendingApproval, ResolvedApproval } from "$lib/types";
  import { pendingApprovals, pendingCount, requestNotificationPermission } from "$lib/stores/hitl";
  import { Badge } from "$lib/components/ui/badge";
  import { Separator } from "$lib/components/ui/separator";
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
    <h1 class="text-2xl font-bold" data-testid="approvals-header">Approvals</h1>
    {#if $pendingCount > 0}
      <Badge variant="destructive" data-testid="approvals-pending-count">{$pendingCount} pending</Badge>
    {/if}
  </div>

  {#if loading}
    <p class="text-sm text-muted-foreground">Loading approvals...</p>
  {:else if error}
    <p class="text-sm text-[hsl(var(--destructive))]">{error}</p>
  {:else}
    <!-- Pending approvals section -->
    <section>
      <h2 class="mb-3 text-lg font-semibold" data-testid="approvals-pending-title">Pending</h2>
      {#if $pendingApprovals.length === 0}
        <div class="flex flex-col items-center justify-center gap-2 rounded-lg border border-dashed py-12">
          <p class="text-muted-foreground">Aucune approbation en attente</p>
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
      <h2 class="mb-3 text-lg font-semibold">History (last 7 days)</h2>
      <ApprovalHistory {history} />
    </section>
  {/if}
</div>
