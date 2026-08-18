<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { fly } from "svelte/transition";
  import { t, locale } from "svelte-i18n";
  import type { TaskSummary } from "$lib/types";
  import { formatDuration } from "$lib/utils";
  import { Sheet, SheetContent } from "$lib/components/ui/sheet";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { addToast } from "$lib/components/ui/toast";
  import {
    ScrollText,
    RefreshCw,
    CheckCircle,
    XCircle,
    Activity,
    Clock,
    AlertTriangle,
    Ban,
    Search,
    X,
    ArrowDownWideNarrow,
    Copy,
  } from "lucide-svelte";
  import { Card } from "$lib/components/ui/card";
  import { createIdentityGuard } from "$lib/utils/identityGuard";

  interface Props {
    agentId: string;
    open: boolean;
    onclose: () => void;
  }

  let { agentId, open, onclose }: Props = $props();

  const FETCH_LIMIT = 200;

  type StatusKey = "all" | "completed" | "failed" | "working" | "submitted" | "input_required" | "canceled";
  type SortKey = "recent" | "oldest" | "longest" | "shortest";

  let taskList = $state<TaskSummary[]>([]);
  let loading = $state(false);
  let error = $state<string | null>(null);

  let searchQuery = $state("");
  let statusFilter = $state<StatusKey>("all");
  let sortBy = $state<SortKey>("recent");

  const STATUS_I18N: Record<string, string> = {
    working: "dashboard.status_working",
    submitted: "dashboard.status_submitted",
    completed: "dashboard.status_completed",
    failed: "dashboard.status_failed",
    input_required: "dashboard.status_approval",
    canceled: "dashboard.status_canceled",
  };

  const STATUS_CONFIG: Record<string, { variant: "success" | "destructive" | "warning" | "info" | "secondary"; icon: typeof Activity }> = {
    completed: { variant: "success", icon: CheckCircle },
    working: { variant: "info", icon: Activity },
    submitted: { variant: "secondary", icon: Clock },
    failed: { variant: "destructive", icon: XCircle },
    input_required: { variant: "warning", icon: AlertTriangle },
    canceled: { variant: "secondary", icon: Ban },
  };

  const STATUS_FILTERS: { key: StatusKey; i18n: string }[] = [
    { key: "all", i18n: "agents.filter_all" },
    { key: "completed", i18n: "dashboard.status_completed" },
    { key: "failed", i18n: "dashboard.status_failed" },
    { key: "working", i18n: "dashboard.status_working" },
    { key: "input_required", i18n: "dashboard.status_approval" },
    { key: "submitted", i18n: "dashboard.status_submitted" },
    { key: "canceled", i18n: "dashboard.status_canceled" },
  ];

  const SORT_OPTIONS: { key: SortKey; i18n: string }[] = [
    { key: "recent", i18n: "agents.sort_recent" },
    { key: "oldest", i18n: "agents.sort_oldest" },
    { key: "longest", i18n: "agents.sort_longest" },
    { key: "shortest", i18n: "agents.sort_shortest" },
  ];

  function shortId(id: string): string {
    return id.slice(0, 8);
  }

  function formatRelativeTime(isoDate: string): string {
    if (!isoDate) return "-";
    const now = Date.now();
    const then = new Date(isoDate).getTime();
    const diffSecs = Math.floor((now - then) / 1000);
    if (diffSecs < 60) return `${diffSecs}s ago`;
    const diffMins = Math.floor(diffSecs / 60);
    if (diffMins < 60) return `${diffMins}min ago`;
    const diffHours = Math.floor(diffMins / 60);
    if (diffHours < 24) return `${diffHours}h ago`;
    const diffDays = Math.floor(diffHours / 24);
    if (diffDays < 7) return `${diffDays}d ago`;
    return new Date(isoDate).toLocaleDateString($locale ?? "en");
  }

  function formatAbsoluteTime(isoDate: string): string {
    if (!isoDate) return "";
    return new Date(isoDate).toLocaleString($locale ?? "en");
  }

  // The sheet keeps its instance across assistants, and the agent it shows can
  // change while a listing is in flight: only the listing still aimed at the
  // assistant on screen may write.
  const agentGuard = createIdentityGuard(() => agentId);

  async function fetchTasks() {
    const ticket = agentGuard.begin();
    loading = true;
    error = null;
    try {
      const result: TaskSummary[] = await invoke("list_tasks", { filter: { agent_id: agentId } });
      if (!ticket.current) return;
      taskList = result.slice(0, FETCH_LIMIT);
    } catch (err: unknown) {
      if (!ticket.current) return;
      error = err instanceof Error ? err.message : String(err);
      taskList = [];
    } finally {
      if (ticket.current) loading = false;
    }
  }

  function resetFilters() {
    searchQuery = "";
    statusFilter = "all";
    sortBy = "recent";
  }

  async function copyLogs() {
    const header = `Apollia · ${$t('agents.logs_title')} · ${agentId} · ${new Date().toISOString()}`;
    const lines = [header, "─".repeat(header.length), ""];
    for (const task of filteredTasks) {
      const status = $t(STATUS_I18N[task.status] ?? "dashboard.status_submitted");
      lines.push(
        `[${status}] ${formatRelativeTime(task.created_at)} (${formatAbsoluteTime(task.created_at)}) · ${formatDuration(task.duration_ms)}`,
        `  ID: ${task.id}`,
        `  ${$t('agents.copy_logs_input_label')}: ${task.input_preview ?? ""}`,
      );
      if (task.output_text) {
        const outLabel = task.status === 'failed' ? $t('agents.task_error_label') : $t('agents.task_output_label');
        lines.push(`  ${outLabel}: ${task.output_text}`);
      }
      lines.push("");
    }
    try {
      await navigator.clipboard.writeText(lines.join("\n"));
      addToast(
        $t("agents.copy_logs_toast", { values: { count: filteredTasks.length } }),
        "success",
      );
    } catch {
      addToast($t("agents.copy_logs_error"), "error");
    }
  }

  const filteredTasks = $derived.by(() => {
    const query = searchQuery.trim().toLowerCase();
    let out = taskList.filter((task) => {
      if (statusFilter !== "all" && task.status !== statusFilter) return false;
      if (query) {
        const haystack = `${task.input_preview ?? ""} ${task.output_text ?? ""}`.toLowerCase();
        if (!haystack.includes(query)) return false;
      }
      return true;
    });

    out = [...out];
    switch (sortBy) {
      case "recent":
        out.sort((a, b) => new Date(b.created_at).getTime() - new Date(a.created_at).getTime());
        break;
      case "oldest":
        out.sort((a, b) => new Date(a.created_at).getTime() - new Date(b.created_at).getTime());
        break;
      case "longest":
        out.sort((a, b) => (b.duration_ms ?? -1) - (a.duration_ms ?? -1));
        break;
      case "shortest":
        out.sort((a, b) => (a.duration_ms ?? Number.MAX_SAFE_INTEGER) - (b.duration_ms ?? Number.MAX_SAFE_INTEGER));
        break;
    }
    return out;
  });

  const hasActiveFilters = $derived(
    searchQuery.trim() !== "" || statusFilter !== "all" || sortBy !== "recent",
  );

  $effect(() => {
    if (open && agentId) {
      void fetchTasks();
    }
  });
</script>

<Sheet {open} {onclose} class="w-full sm:max-w-[520px]">
  <div class="flex h-full flex-col" data-testid="agent-logs-sheet">

    <!-- Header card -->
    <Card class="mx-4 mt-6 overflow-hidden">
      <div class="h-0.5 w-full bg-secondary"></div>
      <div class="px-4 py-3.5 flex items-center justify-between">
        <div class="flex items-center gap-3">
          <div class="flex h-8 w-8 items-center justify-center rounded-lg bg-secondary/10">
            <ScrollText size={15} class="text-secondary" />
          </div>
          <div>
            <h2 class="text-sm font-medium">{$t('agents.logs_title')}</h2>
            <p class="text-[11px] text-muted-foreground">
              {#if hasActiveFilters}
                {filteredTasks.length} / {taskList.length} {$t('agents.tasks_label')}
              {:else}
                {taskList.length} {$t('agents.tasks_label')}
              {/if}
            </p>
          </div>
        </div>
        <div class="flex items-center gap-1">
          <Button
            size="sm"
            variant="ghost"
            class="h-7 w-7 p-0"
            onclick={copyLogs}
            disabled={taskList.length === 0}
            aria-label={$t('agents.copy_logs_button')}
            title={$t('agents.copy_logs_button')}
            data-testid="agent-logs-copy-btn"
          >
            <Copy size={13} class="text-muted-foreground" />
          </Button>
          <Button size="sm" variant="ghost" class="h-7 w-7 p-0" onclick={fetchTasks} disabled={loading} aria-label={$t('common.refresh')}>
            <RefreshCw size={13} class="text-muted-foreground {loading ? 'animate-spin' : ''}" />
          </Button>
        </div>
      </div>
    </Card>

    <!-- Filters -->
    <div class="mx-4 mt-3 space-y-2">
      <!-- Search -->
      <div class="relative">
        <Input
          type="text"
          icon={Search}
          placeholder={$t('agents.filter_search_placeholder')}
          bind:value={searchQuery}
          class="h-8 text-xs"
          aria-label={$t('agents.filter_search_placeholder')}
        />
        {#if searchQuery}
          <Button variant="ghost" size="sm"
            type="button"
            class="absolute right-2 top-1/2 -translate-y-1/2 text-muted-foreground hover:text-foreground"
            onclick={() => (searchQuery = "")}
            aria-label={$t('common.clear')}
          >
            <X size={12} />
          </Button>
        {/if}
      </div>

      <!-- Status chips + sort -->
      <div class="flex items-center justify-between gap-2">
        <div class="flex flex-wrap gap-1">
          {#each STATUS_FILTERS as filter (filter.key)}
            <Button variant="ghost" size="sm"
              type="button"
              onclick={() => (statusFilter = filter.key)}
              class="text-[10px] px-2 py-0.5 rounded-full border transition-colors
                {statusFilter === filter.key
                  ? 'bg-primary text-primary-foreground border-primary'
                  : 'border-border/60 text-muted-foreground hover:border-border hover:text-foreground'}"
            >
              {$t(filter.i18n)}
            </Button>
          {/each}
        </div>

        <div class="flex items-center gap-1 shrink-0">
          <ArrowDownWideNarrow size={11} class="text-muted-foreground/60" />
          <select
            bind:value={sortBy}
            class="text-[10px] bg-transparent border border-border/60 rounded px-1.5 py-0.5 text-muted-foreground hover:text-foreground focus:outline-none focus:ring-1 focus:ring-ring"
            aria-label={$t('agents.sort_label')}
          >
            {#each SORT_OPTIONS as option (option.key)}
              <option value={option.key}>{$t(option.i18n)}</option>
            {/each}
          </select>
        </div>
      </div>
    </div>

    <!-- Task list -->
    <SheetContent padding="flush" class="px-4 pt-3 pb-6">
      {#if loading && taskList.length === 0}
        <div class="flex items-center justify-center py-12">
          <RefreshCw size={16} class="animate-spin text-muted-foreground" />
        </div>
      {:else if error}
        <Card class="rounded-lg px-4 py-3">
          <p class="text-xs text-destructive">{error}</p>
        </Card>
      {:else if taskList.length === 0}
        <Card class="rounded-lg px-4 py-8 text-center">
          <ScrollText size={24} class="mx-auto text-muted-foreground/30 mb-2" />
          <p class="text-xs text-muted-foreground">{$t('agents.no_tasks')}</p>
        </Card>
      {:else if filteredTasks.length === 0}
        <Card class="rounded-lg px-4 py-8 text-center">
          <Search size={24} class="mx-auto text-muted-foreground/30 mb-2" />
          <p class="text-xs text-muted-foreground mb-2">{$t('agents.no_matching_tasks')}</p>
          <Button size="sm" variant="ghost" class="h-6 text-[10px]" onclick={resetFilters}>
            {$t('agents.clear_filters')}
          </Button>
        </Card>
      {:else}
        <Card class="rounded-lg overflow-hidden divide-y divide-border/40">
          {#each filteredTasks as task, i (task.id)}
            {@const cfg = STATUS_CONFIG[task.status] ?? STATUS_CONFIG.submitted}
            {@const IconComponent = cfg.icon}
            {@const isFailed = task.status === 'failed'}
            {@const hasOutput = !!task.output_text}
            <div
              class="flex flex-col gap-1.5 px-3.5 py-3 transition-colors duration-150 hover:bg-primary/5"
              in:fly={{ y: 4, duration: 150, delay: Math.min(i, 10) * 20 }}
            >
              <!-- Row 1: status + timing -->
              <div class="flex items-center justify-between gap-2">
                <div class="flex items-center gap-1.5 min-w-0">
                  <IconComponent
                    size={12}
                    class="{task.status === 'completed' ? 'text-success' :
                      task.status === 'failed' ? 'text-destructive' :
                      task.status === 'working' ? 'text-primary animate-spin' :
                      'text-muted-foreground'} shrink-0"
                  />
                  <Badge variant={cfg.variant} class="text-[9px] px-1.5 py-0 shrink-0">
                    {$t(STATUS_I18N[task.status] ?? "dashboard.status_submitted")}
                  </Badge>
                </div>
                <div class="flex items-center gap-1.5 shrink-0">
                  {#if task.duration_ms !== undefined && task.duration_ms !== null}
                    <span class="text-[10px] text-muted-foreground/50">{formatDuration(task.duration_ms)}</span>
                    <span class="text-muted-foreground/20 text-[10px]">·</span>
                  {/if}
                  <span
                    class="text-[10px] text-muted-foreground/40"
                    title={formatAbsoluteTime(task.created_at)}
                  >
                    {formatRelativeTime(task.created_at)}
                  </span>
                </div>
              </div>

              <!-- Row 2: input preview - what triggered the task -->
              <p
                class="text-[11px] text-foreground/75 leading-snug line-clamp-2"
                title={task.input_preview || undefined}
              >
                {task.input_preview || $t('agents.task_no_description')}
              </p>

              <!-- Row 3: output / error detail -->
              {#if hasOutput}
                <p
                  class="text-[10px] leading-snug line-clamp-2 {isFailed ? 'text-destructive/70' : 'text-muted-foreground/50'}"
                  title={task.output_text || undefined}
                >
                  <span class="font-medium">{isFailed ? $t('agents.task_error_label') : $t('agents.task_output_label')}:</span>
                  {task.output_text}
                </p>
              {/if}

              <!-- UUID as subtle metadata -->
              <code class="text-[9px] text-muted-foreground/25 font-mono">{shortId(task.id)}</code>
            </div>
          {/each}
        </Card>
      {/if}
    </SheetContent>
  </div>
</Sheet>
