<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { fly } from "svelte/transition";
  import { t, locale } from "svelte-i18n";
  import type { TriggerLogEntry } from "$lib/types";
  import { formatRelativeTime } from "$lib/utils";
  import { Sheet, SheetContent } from "$lib/components/ui/sheet";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import {
    History,
    RefreshCw,
    CheckCircle,
    XCircle,
    Activity,
    SkipForward,
    ArrowDownWideNarrow,
  } from "lucide-svelte";
  import { Card } from "$lib/components/ui/card";
  import { Select } from "$lib/components/ui/select";

  interface Props {
    triggerId: string;
    open: boolean;
    onclose: () => void;
  }

  let { triggerId, open, onclose }: Props = $props();

  const FETCH_LIMIT = 200;

  type StatusKey = "all" | "fired" | "skipped" | "error";
  type SortKey = "recent" | "oldest";

  let entries = $state<TriggerLogEntry[]>([]);
  let loading = $state(false);
  let error = $state<string | null>(null);

  let statusFilter = $state<StatusKey>("all");
  let sortBy = $state<SortKey>("recent");

  const STATUS_CONFIG: Record<
    string,
    {
      labelKey: string;
      variant: "success" | "warning" | "destructive" | "secondary";
      icon: typeof Activity;
      iconColor: string;
    }
  > = {
    fired: {
      labelKey: "triggers.status_fired",
      variant: "success",
      icon: CheckCircle,
      iconColor: "text-success",
    },
    skipped: {
      labelKey: "triggers.status_skipped",
      variant: "warning",
      icon: SkipForward,
      iconColor: "text-warning",
    },
    error: {
      labelKey: "triggers.status_error",
      variant: "destructive",
      icon: XCircle,
      iconColor: "text-destructive",
    },
  };

  const STATUS_FILTERS: { key: StatusKey; i18n: string }[] = [
    { key: "all", i18n: "triggers.filter_all" },
    { key: "fired", i18n: "triggers.status_fired" },
    { key: "skipped", i18n: "triggers.status_skipped" },
    { key: "error", i18n: "triggers.status_error" },
  ];

  const SORT_OPTIONS: { key: SortKey; i18n: string }[] = [
    { key: "recent", i18n: "triggers.sort_recent" },
    { key: "oldest", i18n: "triggers.sort_oldest" },
  ];

  function shortId(id: string | null): string {
    if (!id) return "-";
    return id.slice(0, 8);
  }


  function formatAbsoluteTime(isoDate: string): string {
    if (!isoDate) return "";
    return new Date(isoDate).toLocaleString($locale ?? "en");
  }

  async function fetchLogs() {
    loading = true;
    error = null;
    try {
      const result: TriggerLogEntry[] = await invoke("get_trigger_logs", {
        id: triggerId,
        limit: FETCH_LIMIT,
      });
      entries = result;
    } catch (err: unknown) {
      error = err instanceof Error ? err.message : String(err);
      entries = [];
    } finally {
      loading = false;
    }
  }

  function resetFilters() {
    statusFilter = "all";
    sortBy = "recent";
  }

  const filteredEntries = $derived.by(() => {
    let out = entries.filter((entry) => {
      if (statusFilter !== "all" && entry.status !== statusFilter) return false;
      return true;
    });

    out = [...out];
    switch (sortBy) {
      case "recent":
        out.sort((a, b) => new Date(b.fired_at).getTime() - new Date(a.fired_at).getTime());
        break;
      case "oldest":
        out.sort((a, b) => new Date(a.fired_at).getTime() - new Date(b.fired_at).getTime());
        break;
    }
    return out;
  });

  const hasActiveFilters = $derived(statusFilter !== "all" || sortBy !== "recent");

  $effect(() => {
    if (open && triggerId) {
      void fetchLogs();
    }
  });
</script>

<Sheet {open} {onclose} class="w-full sm:max-w-[520px]">
  <div class="flex h-full flex-col" data-testid="trigger-logs-sheet">

    <!-- Header card -->
    <Card class="mx-4 mt-6 overflow-hidden">
      <div class="h-0.5 w-full bg-secondary"></div>
      <div class="px-4 py-3.5 flex items-center justify-between">
        <div class="flex items-center gap-3 min-w-0">
          <div class="flex h-8 w-8 items-center justify-center rounded-lg bg-secondary/10 shrink-0">
            <History size={15} class="text-secondary" />
          </div>
          <div class="min-w-0">
            <h2 class="text-sm font-medium truncate" title={triggerId}>{$t('triggers.logs_title')}</h2>
            <p class="text-caption text-muted-foreground">
              {#if hasActiveFilters}
                {filteredEntries.length} / {entries.length} {$t('triggers.entries_label')}
              {:else}
                {entries.length} {$t('triggers.entries_label')}
              {/if}
            </p>
          </div>
        </div>
        <Button size="sm" variant="ghost" class="h-7 w-7 p-0" onclick={fetchLogs} disabled={loading} aria-label={$t('common.refresh')}>
          <RefreshCw size={13} class="text-muted-foreground {loading ? 'animate-spin' : ''}" />
        </Button>
      </div>
    </Card>

    <!-- Filters -->
    <div class="mx-4 mt-3 flex items-center justify-between gap-2">
      <div class="flex flex-wrap gap-1">
        {#each STATUS_FILTERS as filter (filter.key)}
          <Button variant="ghost" size="sm"
            type="button"
            onclick={() => (statusFilter = filter.key)}
            class="text-caption px-2 py-0.5 rounded-full border transition-colors
              {statusFilter === filter.key
                ? 'bg-primary text-primary-foreground border-primary'
                : 'border-border/60 text-muted-foreground hover:border-border hover:text-foreground'}"
            data-testid="trigger-logs-filter-{filter.key}"
          >
            {$t(filter.i18n)}
          </Button>
        {/each}
      </div>

      <div class="flex items-center gap-1 shrink-0">
        <ArrowDownWideNarrow size={11} class="text-muted-foreground/60" />
        <Select
          bind:value={sortBy}
          class="text-caption bg-transparent border border-border/60 rounded px-1.5 py-0.5 text-muted-foreground hover:text-foreground focus:outline-none focus:ring-1 focus:ring-ring"
          aria-label={$t('triggers.sort_label')}
        >
          {#each SORT_OPTIONS as option (option.key)}
            <option value={option.key}>{$t(option.i18n)}</option>
          {/each}
        </Select>
      </div>
    </div>

    <!-- Entry list -->
    <SheetContent padding="flush" class="px-4 pt-3 pb-6">
      {#if loading && entries.length === 0}
        <div class="flex items-center justify-center py-12">
          <RefreshCw size={16} class="animate-spin text-muted-foreground" />
        </div>
      {:else if error}
        <Card class="rounded-lg px-4 py-3">
          <p class="text-xs text-destructive">{error}</p>
        </Card>
      {:else if entries.length === 0}
        <Card class="rounded-lg px-4 py-8 text-center">
          <History size={24} class="mx-auto text-muted-foreground/30 mb-2" />
          <p class="text-xs text-muted-foreground">{$t('triggers.no_logs')}</p>
        </Card>
      {:else if filteredEntries.length === 0}
        <Card class="rounded-lg px-4 py-8 text-center">
          <History size={24} class="mx-auto text-muted-foreground/30 mb-2" />
          <p class="text-xs text-muted-foreground mb-2">{$t('triggers.no_matching_logs')}</p>
          <Button size="sm" variant="ghost" class="h-6 text-caption" onclick={resetFilters}>
            {$t('triggers.clear_filters')}
          </Button>
        </Card>
      {:else}
        <Card class="rounded-lg overflow-hidden divide-y divide-border/40">
          {#each filteredEntries as entry, i (entry.id)}
            {@const cfg = STATUS_CONFIG[entry.status] ?? STATUS_CONFIG.error}
            {@const IconComponent = cfg.icon}
            {@const isError = entry.status === 'error'}
            <div
              class="flex flex-col gap-1.5 px-3.5 py-3 transition-colors duration-150 hover:bg-primary/5"
              in:fly={{ y: 4, duration: 150, delay: Math.min(i, 10) * 20 }}
            >
              <!-- Row 1: status badge + timing -->
              <div class="flex items-center justify-between gap-2">
                <div class="flex items-center gap-1.5 min-w-0">
                  <IconComponent size={12} class="{cfg.iconColor} shrink-0" />
                  <Badge variant={cfg.variant} class="text-caption px-1.5 py-0 shrink-0">
                    {$t(cfg.labelKey)}
                  </Badge>
                </div>
                <span
                  class="text-caption text-muted-foreground/40 shrink-0"
                  title={formatAbsoluteTime(entry.fired_at)}
                >
                  {formatRelativeTime(entry.fired_at, $locale ?? "en")}
                </span>
              </div>

              <!-- Row 2: agent + task id -->
              <div class="flex items-center justify-between gap-2 text-caption text-muted-foreground/75">
                <span class="truncate" title={entry.agent_name}>
                  <span class="text-muted-foreground/50">{$t('triggers.agent_label')}:</span>
                  {entry.agent_name}
                </span>
                <code class="text-caption text-muted-foreground/50 font-mono shrink-0">
                  {shortId(entry.task_id)}
                </code>
              </div>

              <!-- Row 3: error reason (only for ERROR rows) -->
              {#if isError && entry.reason}
                <p
                  class="text-caption leading-snug line-clamp-2 text-destructive/70"
                  title={entry.reason}
                >
                  <span class="font-medium">{$t('triggers.reason_label')}:</span>
                  {entry.reason}
                </p>
              {/if}
            </div>
          {/each}
        </Card>
      {/if}
    </SheetContent>
  </div>
</Sheet>
