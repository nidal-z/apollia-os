<!--
  Task detail drawer — warm glass design with card sections.
  Layout: Header card → Input card → Result card → Technical (expandable)
-->
<script lang="ts">
  import { t } from "svelte-i18n";
  import type { TaskSummary } from "$lib/types";
  import { tasks } from "$lib/stores/tasks";
  import { uiMode } from "$lib/stores/mode";
  import { Sheet } from "$lib/components/ui/sheet";
  import { Badge } from "$lib/components/ui/badge";
  import { Activity, CheckCircle, XCircle, Clock, AlertTriangle, Ban, Timer, Calendar, ChevronDown, ChevronRight } from "lucide-svelte";
  import TaskTimeline from "./TaskTimeline.svelte";
  import SmartOutput from "../common/SmartOutput.svelte";

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

  let task = $derived<TaskSummary | undefined>($tasks.find((t) => t.id === taskId));
  let isRunning = $derived(task?.status === "working" || task?.status === "submitted");
  let inputTruncated = $derived(task?.input_preview?.includes(TRUNCATION_MARKER) ?? false);
  let outputTruncated = $derived(task?.output_text?.includes(TRUNCATION_MARKER) ?? false);
  let inputNeedsCollapse = $derived((task?.input_preview?.split("\n").length ?? 0) > INPUT_COLLAPSE_LINE_COUNT);
  let inputExpanded = $state(false);
  let technicalExpanded = $state(false);
  let mode = $derived($uiMode);

  const STATUS_I18N: Record<string, string> = {
    working: "dashboard.status_working",
    submitted: "dashboard.status_submitted",
    completed: "dashboard.status_completed",
    failed: "dashboard.status_failed",
    input_required: "dashboard.status_approval",
    canceled: "dashboard.status_canceled",
  };

  function avatarHue(name: string): number {
    return name.split("").reduce((acc, c) => acc + c.charCodeAt(0), 0) % 360;
  }

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
      technicalExpanded = mode === "builder";
    }
  });
</script>

<Sheet open={open} onclose={onclose} class="w-[520px]">
  <div class="flex h-full flex-col" data-testid="task-detail">
    {#if !task}
      <div class="flex-1 flex items-center justify-center">
        <p class="text-xs text-muted-foreground">{$t('tasks.not_found')}</p>
      </div>
    {:else}
      {@const cfg = STATUS_CONFIG[task.status] ?? STATUS_CONFIG.submitted}
      {@const hue = avatarHue(task.agent_name || task.agent_id)}

      <!-- Header card -->
      <div class="mx-4 mt-6 glass-card glass-border rounded-xl overflow-hidden">
        <div class="h-0.5 w-full {task.status === 'completed' ? 'bg-success' : task.status === 'failed' ? 'bg-destructive' : task.status === 'working' ? 'bg-primary' : 'bg-muted'}"></div>
        <div class="px-4 py-4" data-testid="task-detail-header">
          <div class="flex items-center gap-3">
            <!-- Agent avatar -->
            <div
              class="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg text-xs font-semibold text-white"
              style="background: hsl({hue}, 60%, 48%); box-shadow: 0 2px 8px -1px hsla({hue}, 60%, 38%, 0.25);"
            >
              {(task.agent_name || "?").charAt(0).toUpperCase()}
            </div>
            <div class="min-w-0 flex-1">
              <div class="flex items-center gap-2">
                <span class="text-sm font-medium truncate" data-testid="task-detail-title">{task.agent_name || task.agent_id}</span>
                <Badge variant={cfg.variant} class="text-[9px] px-1.5 py-0">{$t(STATUS_I18N[task.status] ?? "dashboard.status_submitted")}</Badge>
              </div>
              <code class="text-[10px] text-muted-foreground/40 font-mono">{task.id.slice(0, 12)}</code>
            </div>
          </div>

          <!-- Meta row -->
          <div class="mt-3 flex items-center gap-4 text-[11px] text-muted-foreground">
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
      </div>

      <!-- Scrollable content -->
      <div class="flex-1 overflow-auto px-4 pt-3 pb-6 space-y-3">

        <!-- Input card -->
        <div class="glass-card glass-border rounded-lg px-4 py-3.5" data-testid="task-detail-input">
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
        </div>

        <!-- Result card -->
        {#if task.output_text}
          <div class="glass-card glass-border rounded-lg px-4 py-3.5" data-testid="task-detail-result">
            <div class="flex items-center gap-2 mb-2">
              <span class="text-[10px] font-medium uppercase tracking-wider text-muted-foreground/50">{$t('tasks.result')}</span>
              {#if outputTruncated}
                <Badge variant="outline" class="text-[9px] px-1 py-0">{$t('tasks.truncated')}</Badge>
              {/if}
            </div>
            <div class="text-xs">
              <SmartOutput output={task.output_text} />
            </div>
          </div>
        {/if}

        <!-- Technical details (expandable card) -->
        <div class="glass-card glass-border rounded-lg overflow-hidden" data-testid="task-detail-technical">
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
            <div class="border-t border-border/40 px-4 py-3 space-y-3">
              <!-- Task ID -->
              <div class="flex items-center gap-2 text-xs">
                <span class="text-muted-foreground/60">{$t('tasks.id_label')}:</span>
                <code class="text-[10px] font-mono text-foreground/70">{task.id}</code>
              </div>

              <!-- Timeline -->
              <div data-testid="task-timeline-section">
                <TaskTimeline taskId={taskId} isRunning={isRunning} />
              </div>
            </div>
          {/if}
        </div>
      </div>
    {/if}
  </div>
</Sheet>
