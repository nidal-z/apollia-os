<script lang="ts">
  import { onMount } from "svelte";
  import { t } from "svelte-i18n";
  import { invoke } from "@tauri-apps/api/core";
  import { Plus, RefreshCw, ListTodo } from "lucide-svelte";
  import type { TaskSummary } from "$lib/types";
  import { tasks, openNewTaskRequested } from "$lib/stores/tasks";
  import { uiMode } from "$lib/stores/mode";
  import { formatRelativeTime } from "$lib/utils";
  import {
    PageHeader,
    Chip,
    StatusDot,
    BtnPrimary,
    BtnSecondary,
    EmptyState,
    TaskRow,
    type Task as DSTask,
    type TaskStatus as DSTaskStatus,
  } from "$lib/components/operator";
  import TaskDetail from "../components/tasks/TaskDetail.svelte";
  import TasksFab from "../components/tasks/TasksFab.svelte";

  // ─── Selection / detail panel ──────────────────────────────────────────────
  let selectedTaskId = $state<string | null>(null);
  let detailOpen = $state(false);

  function handleSelectTask(taskId: string) {
    selectedTaskId = taskId;
    detailOpen = true;
  }

  function handleCloseDetail() {
    detailOpen = false;
  }

  function handleNewTask() {
    openNewTaskRequested.set(Date.now());
  }

  async function handleRefresh() {
    try {
      const result: TaskSummary[] = await invoke("list_tasks", { filter: null });
      tasks.set(result);
    } catch {
      // runtime not ready — keep current state
    }
  }

  // ─── Status filter chips (V3 supervision filters) ──────────────────────────
  type FilterKey = "all" | "todo" | "running" | "approval" | "done" | "error";

  let activeFilter = $state<FilterKey>("all");

  const FILTERS: {
    key: FilterKey;
    label: string;
    color: string;
    /** Backend statuses that map onto this filter bucket. */
    match: (s: TaskSummary["status"]) => boolean;
  }[] = [
    {
      key: "all",
      label: "Toutes",
      color: "hsl(var(--muted-foreground))",
      match: () => true,
    },
    {
      key: "todo",
      label: "À faire",
      color: "hsl(var(--info))",
      match: (s) => s === "submitted",
    },
    {
      key: "running",
      label: "En cours",
      color: "hsl(var(--primary))",
      match: (s) => s === "working",
    },
    {
      key: "approval",
      label: "Approbation",
      color: "hsl(var(--warning))",
      match: (s) => s === "input_required",
    },
    {
      key: "done",
      label: "Terminé",
      color: "hsl(var(--success))",
      match: (s) => s === "completed",
    },
    {
      key: "error",
      label: "Erreur",
      color: "hsl(var(--destructive))",
      match: (s) => s === "failed",
    },
  ];

  // ─── Derived data ──────────────────────────────────────────────────────────
  const counts = $derived.by(() => {
    const out: Record<FilterKey, number> = {
      all: 0,
      todo: 0,
      running: 0,
      approval: 0,
      done: 0,
      error: 0,
    };
    for (const task of $tasks) {
      out.all += 1;
      for (const f of FILTERS) {
        if (f.key === "all") continue;
        if (f.match(task.status)) out[f.key] += 1;
      }
    }
    return out;
  });

  const filteredTasks = $derived.by(() => {
    const filter = FILTERS.find((f) => f.key === activeFilter)!;
    return $tasks.filter((task) => filter.match(task.status));
  });

  /** Tasks currently in flight (submitted + working) — used by the headline. */
  const inFlightCount = $derived(counts.todo + counts.running);

  const headlineTitle = $derived(
    inFlightCount === 0
      ? "Aucune tâche en cours"
      : inFlightCount === 1
        ? "1 tâche en cours"
        : `${inFlightCount} tâches en cours`,
  );

  const headlineSubtitle = $derived(
    $uiMode === "operator"
      ? "Ce que font les agents pour vous, en ce moment et dans les prochaines minutes."
      : ($t("tasks.subtitle") as string),
  );

  // ─── TaskSummary → DS Task adapter ────────────────────────────────────────
  const STATUS_MAP: Record<TaskSummary["status"], DSTaskStatus> = {
    submitted: "queued",
    working: "running",
    completed: "completed",
    failed: "failed",
    input_required: "awaiting_approval",
    canceled: "cancelled",
  };

  function toDSTask(task: TaskSummary): DSTask {
    const progress =
      task.status === "completed"
        ? 100
        : task.status === "working"
          ? 45
          : task.status === "input_required"
            ? 20
            : task.status === "failed"
              ? 60
              : 0;
    return {
      id: task.id,
      title: task.output_text || task.input_preview || "(sans titre)",
      agent: task.agent_name || task.agent_id,
      status: STATUS_MAP[task.status] ?? "queued",
      progress,
      started: formatRelativeTime(task.created_at),
    };
  }

  onMount(() => {
    // Page mount — store is already populated by sse.ts watchdog/refresh.
  });
</script>

<div class="mx-auto w-full max-w-6xl" data-testid="tasks-page">
  <PageHeader
    kicker="MON TRAVAIL"
    title={headlineTitle}
    subtitle={headlineSubtitle}
  >
    {#snippet actions()}
      <BtnSecondary onclick={handleRefresh}>
        {#snippet icon()}
          <RefreshCw size={12} />
        {/snippet}
        Actualiser
      </BtnSecondary>
      <BtnPrimary onclick={handleNewTask}>
        {#snippet icon()}
          <Plus size={12} />
        {/snippet}
        Assigner une tâche
      </BtnPrimary>
    {/snippet}
  </PageHeader>

  <!-- Status filter chip row (V3 supervision filters) -->
  <div
    class="flex flex-wrap items-center gap-2 px-8 pt-5 pb-4"
    role="tablist"
    aria-label="Filtres de statut"
    data-testid="tasks-filter-bar"
  >
    {#each FILTERS as f (f.key)}
      {@const isActive = activeFilter === f.key}
      <button
        type="button"
        role="tab"
        aria-selected={isActive}
        onclick={() => (activeFilter = f.key)}
        class="cursor-pointer border-0 bg-transparent p-0"
        data-testid="tasks-filter-{f.key}"
        data-active={isActive}
      >
        <Chip
          tone={isActive ? "primary" : "neutral"}
          outline={!isActive}
          size="md"
        >
          {#snippet icon()}
            <StatusDot color={f.color} glow={isActive && f.key === "running"} />
          {/snippet}
          {f.label} · {counts[f.key]}
        </Chip>
      </button>
    {/each}
  </div>

  <!-- Task table -->
  <div class="px-8 pb-10">
    {#if filteredTasks.length === 0}
      <div
        class="rounded-xl border border-border/60 bg-card overflow-hidden"
        data-testid="tasks-empty"
      >
        <EmptyState
          title={activeFilter === "all"
            ? "Aucune tâche pour l'instant"
            : "Aucune tâche dans ce statut"}
          desc={activeFilter === "all"
            ? "Assignez une tâche à un agent pour démarrer la supervision."
            : "Changez de filtre pour explorer les autres tâches."}
        >
          {#snippet icon()}
            <ListTodo size={22} />
          {/snippet}
          {#snippet action()}
            {#if activeFilter === "all"}
              <BtnPrimary onclick={handleNewTask}>
                {#snippet icon()}
                  <Plus size={12} />
                {/snippet}
                Assigner une tâche
              </BtnPrimary>
            {:else}
              <BtnSecondary onclick={() => (activeFilter = "all")}>
                Voir toutes les tâches
              </BtnSecondary>
            {/if}
          {/snippet}
        </EmptyState>
      </div>
    {:else}
      <div
        class="rounded-xl border border-border/60 bg-card overflow-hidden"
        data-testid="tasks-table"
      >
        <!-- Column headers -->
        <div
          class="px-4 py-2.5 border-b border-border/60 flex items-center gap-2.5 text-[10.5px] uppercase tracking-[1px] font-semibold text-muted-foreground/70"
        >
          <div class="flex-[2] min-w-0">Tâche</div>
          <div class="w-[140px]">Agent</div>
          <div class="w-[140px]">Progrès</div>
          <div class="w-[120px]">Statut</div>
          <div class="w-[90px] text-right">Démarré</div>
        </div>

        {#each filteredTasks as task (task.id)}
          <TaskRow
            task={toDSTask(task)}
            onclick={() => handleSelectTask(task.id)}
          />
        {/each}
      </div>
    {/if}
  </div>
</div>

<!-- Mobile FAB (operator). Desktop uses the top-right button in the header. -->
<TasksFab onclick={handleNewTask} />

{#if selectedTaskId}
  <TaskDetail taskId={selectedTaskId} open={detailOpen} onclose={handleCloseDetail} />
{/if}
