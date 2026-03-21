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

  const STATUS_VARIANT: Record<string, "default" | "secondary" | "destructive" | "outline"> = {
    running: "outline",
    waiting_approval: "outline",
    completed: "default",
    failed: "destructive",
  };

  const STATUS_EXTRA_CLASS: Record<string, string> = {
    running: "animate-pulse border-info text-info",
    waiting_approval: "border-[var(--apollia-warning)] text-[var(--apollia-warning)]",
    completed: "bg-[var(--apollia-success)] text-white",
    failed: "",
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

<div
  class="flex items-center gap-3 rounded-md border px-4 py-3 text-sm transition-colors hover:bg-muted/50"
>
  <code class="shrink-0 text-xs text-muted-foreground">{shortId(run.run_id)}</code>
  <span class="min-w-0 flex-1 truncate font-medium">{run.pipeline_id}</span>
  <Badge
    variant={STATUS_VARIANT[run.status] ?? "secondary"}
    class="shrink-0 text-[10px] {STATUS_EXTRA_CLASS[run.status] ?? ''}"
  >
    {run.status === "waiting_approval" ? "WAITING" : run.status.toUpperCase()}
  </Badge>
  <span class="shrink-0 text-xs text-muted-foreground">{formatElapsed(run.started_at)}</span>
  <Button size="sm" variant="outline" onclick={() => ondetail(run.run_id)}>{$t('pipelines.detail_button')}</Button>
</div>
