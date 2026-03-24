<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
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
    if (stats.hit_rate_pct > 50) return "text-green-400";
    if (stats.hit_rate_pct >= 20) return "text-amber-400";
    return "text-red-400";
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
      addToast(`Cache purgé (${previousTotal} entrées supprimées)`, "success");
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
    <h2 class="text-lg font-semibold">Plan Cache</h2>
    <Button
      variant="destructive"
      size="sm"
      onclick={openPurgeDialog}
      disabled={loading || isEmpty || clearing}
      data-testid="plan-cache-clear-btn"
    >
      <Trash2 class="mr-2 h-4 w-4" />
      Purger le cache
    </Button>
  </div>

  <!-- Loading skeleton -->
  {#if loading}
    <div class="grid grid-cols-1 gap-4 md:grid-cols-2 lg:grid-cols-4">
      {#each Array(4) as _}
        <Skeleton class="h-24 rounded-xl bg-white/5" />
      {/each}
    </div>

  <!-- Empty state -->
  {:else if isEmpty}
    <EmptyState
      icon={Archive}
      title="Aucun plan en cache"
      subtitle="Les plans seront mis en cache automatiquement lors des prochaines exécutions orchestrées."
      page="plan-cache"
    />

  <!-- Stats cards -->
  {:else if stats}
    <div class="grid grid-cols-1 gap-4 md:grid-cols-2 lg:grid-cols-4">
      <!-- Total Entries -->
      <div
        class="rounded-xl border border-white/10 bg-white/5 p-4 backdrop-blur-xl"
        data-testid="plan-cache-total"
      >
        <div class="flex items-center gap-2 text-xs text-muted-foreground">
          <Database class="h-4 w-4" />
          Total Entries
        </div>
        <p class="mt-2 text-2xl font-semibold">{stats.total_entries}</p>
      </div>

      <!-- Hit Rate -->
      <div
        class="rounded-xl border border-white/10 bg-white/5 p-4 backdrop-blur-xl"
        data-testid="plan-cache-hit-rate"
      >
        <div class="flex items-center gap-2 text-xs text-muted-foreground">
          <TrendingUp class="h-4 w-4" />
          Hit Rate
        </div>
        <p class="mt-2 text-2xl font-semibold {hitRateColorClass}">
          {#if stats.cache_hits + stats.cache_misses === 0}
            N/A
          {:else}
            {stats.hit_rate_pct.toFixed(1)}%
          {/if}
        </p>
      </div>

      <!-- Cache Hits -->
      <div
        class="rounded-xl border border-white/10 bg-white/5 p-4 backdrop-blur-xl"
        data-testid="plan-cache-hits"
      >
        <div class="flex items-center gap-2 text-xs text-muted-foreground">
          <CheckCircle class="h-4 w-4" />
          Cache Hits
        </div>
        <p class="mt-2 text-2xl font-semibold">{stats.cache_hits}</p>
      </div>

      <!-- Cache Misses -->
      <div
        class="rounded-xl border border-white/10 bg-white/5 p-4 backdrop-blur-xl"
        data-testid="plan-cache-misses"
      >
        <div class="flex items-center gap-2 text-xs text-muted-foreground">
          <XCircle class="h-4 w-4" />
          Cache Misses
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
    title="Purger le cache de plans"
    message="Voulez-vous vider les {stats?.total_entries ?? 0} entrées du cache ? Les prochaines exécutions orchestrées devront régénérer leurs plans via LLM."
    confirmLabel="Purger"
    cancelLabel="Annuler"
    loading={clearing}
    data-testid="plan-cache-purge-dialog"
  />
</div>
