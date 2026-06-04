<!--
  Task detail drawer - glass panel, no absolute positioning.
  Layout: header strip (sticky) → scrollable cards
-->
<script lang="ts">
  import { t } from "svelte-i18n";
  import type { TaskSummary } from "$lib/types";
  import { tasks } from "$lib/stores/tasks";
  import { uiMode } from "$lib/stores/mode";
  import { Sheet, SheetContent } from "$lib/components/ui/sheet";
  import { Avatar } from "$lib/components/ui/avatar";
  import { Badge } from "$lib/components/ui/badge";
  import {
    Activity, CheckCircle, XCircle, Clock, AlertTriangle, Ban,
    Timer, Calendar, ChevronDown, ChevronRight,
    Maximize2, Minimize2, X,
  } from "lucide-svelte";
  import { Separator } from "$lib/components/ui/separator";
  import ExecutionTrace from "../observability/ExecutionTrace.svelte";
  import SmartOutput from "../common/SmartOutput.svelte";
  import { BuilderOnly } from "$lib/components/shared";
  import { Card } from "$lib/components/ui/card";

  interface Props {
    taskId: string;
    open: boolean;
    onclose: () => void;
  }

  let { taskId, open, onclose }: Props = $props();

  const TRUNCATION_MARKER = "[TRONQUE]";
  const INPUT_COLLAPSE_LINE_COUNT = 3;

  const STATUS_CONFIG: Record<string, { variant: "success" | "destructive" | "warning" | "info" | "secondary"; icon: typeof Activity }> = {
    completed: { variant: "success", icon: CheckCircle },
    working: { variant: "info", icon: Activity },
    submitted: { variant: "secondary", icon: Clock },
    failed: { variant: "destructive", icon: XCircle },
    input_required: { variant: "warning", icon: AlertTriangle },
    canceled: { variant: "secondary", icon: Ban },
  };

  const STATUS_I18N: Record<string, string> = {
    working: "dashboard.status_working",
    submitted: "dashboard.status_submitted",
    completed: "dashboard.status_completed",
    failed: "dashboard.status_failed",
    input_required: "dashboard.status_approval",
    canceled: "dashboard.status_canceled",
  };

  let task = $derived<TaskSummary | undefined>($tasks.find((t) => t.id === taskId));
  let isRunning = $derived(task?.status === "working" || task?.status === "submitted");
  let inputTruncated = $derived(task?.input_preview?.includes(TRUNCATION_MARKER) ?? false);
  let outputTruncated = $derived(task?.output_text?.includes(TRUNCATION_MARKER) ?? false);
  let inputNeedsCollapse = $derived((task?.input_preview?.split("\n").length ?? 0) > INPUT_COLLAPSE_LINE_COUNT);
  let inputExpanded = $state(false);
  let technicalExpanded = $state(false);
  let wideMode = $state(false);
  let mode = $derived($uiMode);

  function formatDurationLong(ms: number | undefined): string {
    if (ms === undefined || ms === null) return "-";
    if (ms < 1000) return `${ms}ms`;
    if (ms < 60_000) return `${(ms / 1000).toFixed(1)}s`;
    const mins = Math.floor(ms / 60_000);
    const secs = Math.floor((ms % 60_000) / 1000);
    return `${mins}m ${secs}s`;
  }

  function formatDate(iso: string): string {
    if (!iso) return "-";
    return new Date(iso).toLocaleString();
  }

  $effect(() => {
    if (open) {
      inputExpanded = false;
      technicalExpanded = false;
      wideMode = false;
    }
  });
</script>

<Sheet open={open} onclose={onclose} class={wideMode ? "w-full sm:max-w-[760px]" : "w-full sm:max-w-[520px]"}>
  <div class="flex h-full min-h-0 flex-col" data-testid="task-detail">
    {#if !task}
      <div class="flex flex-1 items-center justify-center">
        <p class="text-xs text-muted-foreground">{$t('tasks.not_found')}</p>
      </div>
    {:else}
      {@const cfg = STATUS_CONFIG[task.status] ?? STATUS_CONFIG.submitted}

      <!-- ── Header strip (shrink-0, never scrolls) ───────────────────── -->
      <div
        class="shrink-0 border-b border-border/40"
        data-testid="task-detail-header"
      >
        <!-- Status bar -->
        <div class="h-0.5 w-full {task.status === 'completed' ? 'bg-success' : task.status === 'failed' ? 'bg-destructive' : task.status === 'working' ? 'bg-primary animate-pulse' : 'bg-muted'}"></div>

        <!-- Title row: avatar + name + status | expand + close -->
        <div class="flex items-center gap-3 px-4 pt-4 pb-0">
          <Avatar name={task.agent_name || task.agent_id} fallback={(task.agent_name || "?").charAt(0).toUpperCase()} size="md" ring />

          <div class="min-w-0 flex-1">
            <span class="block text-sm font-medium truncate" data-testid="task-detail-title">
              {task.agent_name || task.agent_id}
            </span>
            <BuilderOnly>
              <code class="text-[10px] text-muted-foreground/40 font-mono">{task.id.slice(0, 12)}</code>
            </BuilderOnly>
          </div>

          <Badge variant={cfg.variant} class="shrink-0 text-[9px] px-1.5 py-0">
            {$t(STATUS_I18N[task.status] ?? "dashboard.status_submitted")}
          </Badge>

          <!-- Panel controls (inline, no absolute) -->
          <div class="flex shrink-0 items-center gap-0.5">
            <button
              class="rounded-md p-1.5 text-muted-foreground hover:bg-white/10 hover:text-foreground transition-colors"
              onclick={() => wideMode = !wideMode}
              title={wideMode ? "Réduire" : "Agrandir"}
              data-testid="panel-expand-btn"
            >
              {#if wideMode}
                <Minimize2 size={15} />
              {:else}
                <Maximize2 size={15} />
              {/if}
            </button>
            <button
              class="rounded-md p-1.5 text-muted-foreground hover:bg-white/10 hover:text-foreground transition-colors"
              onclick={onclose}
              aria-label={$t("a11y.close")}
              data-testid="sheet-close"
            >
              <X size={15} />
            </button>
          </div>
        </div>

        <!-- Meta row: duration + date -->
        <div class="flex items-center gap-4 px-4 pt-2 pb-3 text-[11px] text-muted-foreground">
          <span class="flex items-center gap-1">
            <Timer size={11} />
            {formatDurationLong(task.duration_ms)}
          </span>
          <span class="flex items-center gap-1">
            <Calendar size={11} />
            {formatDate(task.created_at)}
          </span>
        </div>
      </div>

      <!-- ── Scrollable content ─────────────────────────────────────────── -->
      <SheetContent padding="flush" class="px-4 py-4 space-y-3">

        <!-- Input card -->
        <Card class="rounded-lg px-4 py-3.5" data-testid="task-detail-input">
          <div class="flex items-center gap-2 mb-2">
            <span class="text-[10px] font-medium uppercase tracking-wider text-muted-foreground/50">{$t('tasks.input')}</span>
            {#if inputTruncated}
              <Badge variant="outline" class="text-[9px] px-1 py-0">{$t('tasks.truncated')}</Badge>
            {/if}
          </div>
          {#if inputNeedsCollapse && !inputExpanded}
            <p class="line-clamp-3 whitespace-pre-wrap text-xs text-foreground/80 leading-relaxed">
              {task.input_preview || $t('common.no_input')}
            </p>
            <button
              class="mt-1.5 text-[11px] text-primary hover:text-primary/80 transition-colors"
              onclick={() => inputExpanded = true}
            >
              {$t('tasks.show_details')}
            </button>
          {:else}
            <p class="whitespace-pre-wrap text-xs text-foreground/80 leading-relaxed">
              {task.input_preview || $t('common.no_input')}
            </p>
            {#if inputNeedsCollapse}
              <button
                class="mt-1.5 text-[11px] text-primary hover:text-primary/80 transition-colors"
                onclick={() => inputExpanded = false}
              >
                {$t('tasks.hide_details')}
              </button>
            {/if}
          {/if}
        </Card>

        <!-- Result / Error card -->
        {#if task.output_text}
          <Card class="rounded-lg px-4 py-3.5 {task.status === 'failed' ? 'border-destructive/30' : ''}" data-testid="task-detail-result">
            <div class="flex items-center gap-2 mb-2">
              {#if task.status === 'failed'}
                <XCircle size={11} class="text-destructive shrink-0" />
                <span class="text-[10px] font-medium uppercase tracking-wider text-destructive/70">{$t('tasks.error')}</span>
              {:else}
                <span class="text-[10px] font-medium uppercase tracking-wider text-muted-foreground/50">{$t('tasks.result')}</span>
              {/if}
              {#if outputTruncated}
                <Badge variant="outline" class="text-[9px] px-1 py-0">{$t('tasks.truncated')}</Badge>
              {/if}
            </div>
            <div class="text-xs {task.status === 'failed' ? 'text-destructive/80 font-mono break-all' : ''}">
              {#if task.status === 'failed'}
                {task.output_text}
              {:else}
                <SmartOutput output={task.output_text} />
              {/if}
            </div>
          </Card>
        {/if}

        <!-- Timeline card (always visible) -->
        <Card class="rounded-lg overflow-hidden" data-testid="task-detail-timeline">
          <div class="flex items-center gap-2 px-4 py-3 border-b border-border/40">
            <span class="text-[10px] font-medium uppercase tracking-wider text-muted-foreground/50">{$t('tasks.timeline')}</span>
            {#if isRunning}
              <span class="ml-1 h-1.5 w-1.5 rounded-full bg-primary animate-pulse"></span>
            {/if}
          </div>
          <div class="px-4 py-3" data-testid="task-timeline-section">
            <!-- Remplace TaskTimeline.svelte par la vue
                 conversation-like event-sourced. Le skin operator/builder
                 est hérité du store $uiMode global. -->
            <ExecutionTrace
              {taskId}
              context="task"
              mode={isRunning ? "live" : "replay"}
            />
          </div>
        </Card>

        <!-- Technical details (builder only) -->
        {#if mode === "builder"}
          <Card class="rounded-lg overflow-hidden" data-testid="task-detail-technical">
            <button
              class="flex w-full items-center gap-2 px-4 py-3 text-left transition-colors hover:bg-primary/5"
              onclick={() => technicalExpanded = !technicalExpanded}
            >
              {#if technicalExpanded}
                <ChevronDown size={12} class="text-muted-foreground/50" />
              {:else}
                <ChevronRight size={12} class="text-muted-foreground/50" />
              {/if}
              <span class="text-[10px] font-medium uppercase tracking-wider text-muted-foreground/50">{$t('tasks.technical_details')}</span>
            </button>
            {#if technicalExpanded}
              <div class="px-4 py-3">
                <Separator class="mb-3" />
                <div class="flex items-center gap-2 text-xs">
                  <span class="text-muted-foreground/60">{$t('tasks.id_label')}:</span>
                  <code class="text-[10px] font-mono text-foreground/70 break-all">{task.id}</code>
                </div>
              </div>
            {/if}
          </Card>
        {/if}

      </SheetContent>
    {/if}
  </div>
</Sheet>
