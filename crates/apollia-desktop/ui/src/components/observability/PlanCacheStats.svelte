<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import type { PlanCacheStats } from "$lib/types";
  import { Button } from "$lib/components/ui/button";
  import { Skeleton } from "$lib/components/ui/skeleton";
  import ConfirmDialog from "$lib/components/ui/dialog/ConfirmDialog.svelte";
  import EmptyState from "../common/EmptyState.svelte";
  import { addToast } from "$lib/components/ui/toast/store";
  import {
    Database,
    CheckCircle,
    XCircle,
    Trash2,
    TrendingUp,
    Archive,
  } from "lucide-svelte";

  let stats = $state<PlanCacheStats | null>(null);
  let loading = $state(true);
  let clearing = $state(false);
  let confirmVisible = $state(false);

  let isEmpty = $derived(stats !== null && stats.total_entries === 0);

  let hitRateColorClass = $derived.by(() => {
    if (stats === null) return "text-muted-foreground";
    if (stats.cache_hits + stats.cache_misses === 0)
      return "text-muted-foreground";
    if (stats.hit_rate_pct > 50) return "text-success";
    if (stats.hit_rate_pct >= 20) return "text-warning";
    return "text-destructive";
  });

  async function loadStats(): Promise<void> {
    try {
      stats = await invoke<PlanCacheStats>("get_plan_cache_stats");
    } catch (err: unknown) {
      const message = err instanceof Error ? err.message : String(err);
      addToast(message, "error");
    } finally {
      loading = false;
    }
  }

  function openPurgeDialog(): void {
    confirmVisible = true;
  }

  function closePurgeDialog(): void {
    confirmVisible = false;
  }

  async function confirmPurge(): Promise<void> {
    const previousTotal = stats?.total_entries ?? 0;
    clearing = true;
    try {
      await invoke("clear_plan_cache");
      addToast($t("observability.plan_cache.toast_purged", { values: { count: previousTotal } }), "success");
      await loadStats();
    } catch (err: unknown) {
      const message = err instanceof Error ? err.message : String(err);
      addToast(message, "error");
    } finally {
      clearing = false;
      confirmVisible = false;
    }
  }

  onMount(() => {
    void loadStats();
  });
</script>

<div data-testid="plan-cache-stats" class="space-y-6">
  <!-- Header with purge button -->
  <div class="flex items-center justify-between">
    <h2 class="text-lg font-semibold">{$t("observability.plan_cache.title")}</h2>
    <Button
      variant="destructive"
      size="sm"
      onclick={openPurgeDialog}
      disabled={loading || isEmpty || clearing}
      data-testid="plan-cache-clear-btn"
    >
      <Trash2 class="mr-2 h-4 w-4" />
      {$t("observability.plan_cache.purge_button")}
    </Button>
  </div>

  <!-- Loading skeleton -->
  {#if loading}
    <div class="grid grid-cols-1 gap-4 md:grid-cols-2 lg:grid-cols-4">
      {#each Array(4) as _}
        <Skeleton class="h-24 rounded-xl" />
      {/each}
    </div>

  <!-- Empty state -->
  {:else if isEmpty}
    <EmptyState
      icon={Archive}
      title={$t("observability.plan_cache.empty_title")}
      subtitle={$t("observability.plan_cache.empty_subtitle")}
      page="plan-cache"
    />

  <!-- Stats cards -->
  {:else if stats}
    <div class="grid grid-cols-1 gap-4 md:grid-cols-2 lg:grid-cols-4">
      <!-- Total Entries -->
      <div
        class="rounded-xl glass-card glass-border p-4"
        data-testid="plan-cache-total"
      >
        <div class="flex items-center gap-2 text-xs text-muted-foreground">
          <Database class="h-4 w-4" />
          {$t("observability.plan_cache.total_entries")}
        </div>
        <p class="mt-2 text-2xl font-semibold">{stats.total_entries}</p>
      </div>

      <!-- Hit Rate -->
      <div
        class="rounded-xl glass-card glass-border p-4"
        data-testid="plan-cache-hit-rate"
      >
        <div class="flex items-center gap-2 text-xs text-muted-foreground">
          <TrendingUp class="h-4 w-4" />
          {$t("observability.plan_cache.hit_rate")}
        </div>
        <p class="mt-2 text-2xl font-semibold {hitRateColorClass}">
          {#if stats.cache_hits + stats.cache_misses === 0}
            {$t("common.na")}
          {:else}
            {stats.hit_rate_pct.toFixed(1)}%
          {/if}
        </p>
      </div>

      <!-- Cache Hits -->
      <div
        class="rounded-xl glass-card glass-border p-4"
        data-testid="plan-cache-hits"
      >
        <div class="flex items-center gap-2 text-xs text-muted-foreground">
          <CheckCircle class="h-4 w-4" />
          {$t("observability.plan_cache.cache_hits")}
        </div>
        <p class="mt-2 text-2xl font-semibold">{stats.cache_hits}</p>
      </div>

      <!-- Cache Misses -->
      <div
        class="rounded-xl glass-card glass-border p-4"
        data-testid="plan-cache-misses"
      >
        <div class="flex items-center gap-2 text-xs text-muted-foreground">
          <XCircle class="h-4 w-4" />
          {$t("observability.plan_cache.cache_misses")}
        </div>
        <p class="mt-2 text-2xl font-semibold">{stats.cache_misses}</p>
      </div>
    </div>
  {/if}

  <!-- Purge confirmation dialog -->
  <ConfirmDialog
    open={confirmVisible}
    onclose={closePurgeDialog}
    onconfirm={confirmPurge}
    title={$t("observability.plan_cache.dialog_title")}
    message={$t("observability.plan_cache.dialog_message", { values: { count: stats?.total_entries ?? 0 } })}
    confirmLabel={$t("observability.plan_cache.dialog_confirm")}
    cancelLabel={$t("common.cancel")}
    loading={clearing}
    data-testid="plan-cache-purge-dialog"
  />
</div>
