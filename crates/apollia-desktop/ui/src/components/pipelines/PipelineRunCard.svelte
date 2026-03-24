<script lang="ts">
  import { t } from "svelte-i18n";
  import type { PipelineRunSummary } from "$lib/types";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";

  interface Props {
    run: PipelineRunSummary;
    ondetail: (runId: string) => void;
  }

  let { run, ondetail }: Props = $props();

  const STATUS_BAR_COLOR: Record<string, string> = {
    running: "bg-primary",
    completed: "bg-emerald-500",
    failed: "bg-destructive",
    waiting_approval: "bg-amber-500",
  };

  const STATUS_BADGE_VARIANT: Record<string, "default" | "success" | "destructive" | "warning" | "info"> = {
    running: "info",
    completed: "success",
    failed: "destructive",
    waiting_approval: "warning",
  };

  function shortId(id: string): string {
    return id.slice(0, 8);
  }

  function formatElapsed(startedAt: string | null): string {
    if (!startedAt) return "-";
    const now = Date.now();
    const start = new Date(startedAt).getTime();
    const diffMs = now - start;
    if (diffMs < 1000) return "<1s";
    const secs = Math.floor(diffMs / 1000);
    if (secs < 60) return `${secs}s`;
    const mins = Math.floor(secs / 60);
    if (mins < 60) return `${mins}m ${secs % 60}s`;
    const hours = Math.floor(mins / 60);
    return `${hours}h ${mins % 60}m`;
  }
</script>

<div class="glass-card-hover relative overflow-hidden" data-testid="pipeline-run-card">
  <!-- Status accent bar -->
  <div
    class="h-0.5 w-full {STATUS_BAR_COLOR[run.status] ?? 'bg-muted-foreground/20'}"
    data-testid="pipeline-run-status-bar"
  ></div>

  <div class="px-3.5 pt-3 pb-2.5">
    <!-- Header -->
    <div class="flex items-center justify-between">
      <div class="flex items-center gap-2">
        <h3 class="text-[13px] font-medium">{run.pipeline_id}</h3>
        <code class="text-[11px] text-muted-foreground">{shortId(run.run_id)}</code>
      </div>
      <Badge variant={STATUS_BADGE_VARIANT[run.status] ?? "secondary"}>
        {run.status === "waiting_approval" ? "WAITING" : run.status.toUpperCase()}
      </Badge>
    </div>

    <!-- Run info -->
    <p class="text-[11px] text-muted-foreground mt-1">
      {$t("pipelines.started")}: {formatElapsed(run.started_at)}
    </p>

    <!-- Actions -->
    <div class="flex gap-2 mt-3">
      <Button size="sm" variant="outline" onclick={() => ondetail(run.run_id)} data-testid="pipeline-run-detail-btn">
        {$t("pipelines.detail_button")}
      </Button>
    </div>
  </div>
</div>
