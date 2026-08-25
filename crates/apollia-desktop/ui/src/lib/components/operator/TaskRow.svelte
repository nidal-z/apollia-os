<script lang="ts">
  import { Shield, Sparkles } from "lucide-svelte";
  import StatusDot from "./StatusDot.svelte";
  import ListRow from "./ListRow.svelte";
  import { Badge } from "$lib/components/ui/badge";

  export type TaskStatus =
    | "queued"
    | "running"
    | "awaiting_approval"
    | "completed"
    | "failed"
    | "paused"
    | "scheduled"
    | "cancelled"
    | "review";

  export type Task = {
    id?: string;
    title: string;
    agent: string;
    status: TaskStatus;
    /** 0-100. */
    progress?: number;
    /** Display label (e.g. "il y a 2m"). */
    started?: string;
    eta?: string;
    /** When status === "failed". */
    errorMsg?: string;
    /** Builder-only metadata to render in expanded row. */
    builderMeta?: string;
  };

  interface Props {
    task: Task;
    /** Show builder/technical detail row underneath. */
    builder?: boolean;
    onclick?: (e: MouseEvent | KeyboardEvent) => void;
  }

  let { task, builder = false, onclick }: Props = $props();

  const STATUS: Record<
    TaskStatus,
    { label: string; tone: "primary" | "warning" | "success" | "danger" | "info" | "neutral" }
  > = {
    queued: { label: "en file", tone: "neutral" },
    running: { label: "en cours", tone: "primary" },
    awaiting_approval: { label: "attend accord", tone: "warning" },
    completed: { label: "terminée", tone: "success" },
    review: { label: "à valider", tone: "success" },
    failed: { label: "erreur", tone: "danger" },
    paused: { label: "en pause", tone: "neutral" },
    scheduled: { label: "planifié", tone: "info" },
    cancelled: { label: "annulé", tone: "neutral" },
  };

  const cfg = $derived(STATUS[task.status]);
  const isCancelled = $derived(task.status === "cancelled");
  const isScheduled = $derived(task.status === "scheduled");
  const showProgress = $derived(
    task.progress !== undefined && task.progress > 0 && !isCancelled && !isScheduled,
  );

  const progressColor = $derived(
    task.status === "failed"
      ? "hsl(var(--destructive))"
      : task.status === "completed" || task.status === "review"
        ? "hsl(var(--success))"
        : task.status === "awaiting_approval"
          ? "hsl(var(--warning))"
          : "hsl(var(--primary))",
  );
</script>

<ListRow
  align="center"
  state={isCancelled ? "dimmed" : "default"}
  onclick={onclick}
  class="text-[12px]"
  data-testid={`task-row-${task.id ?? task.title}`}
>
  <div class="flex-[2] min-w-0">
    <div
      class="font-medium text-foreground flex items-center gap-1.5"
      style:text-decoration={isCancelled ? "line-through" : "none"}
    >
      <span class="truncate">{task.title}</span>
      {#if task.status === "awaiting_approval"}
        <Shield size={11} class="text-warning-a11y shrink-0" />
      {/if}
    </div>
  </div>
  <div class="w-[140px] flex items-center gap-1.5 min-w-0">
    <div
      class="w-[18px] h-[18px] rounded-md inline-flex items-center justify-center shrink-0"
      style="background: linear-gradient(135deg, hsl(var(--primary)), hsl(var(--secondary)));"
    >
      <Sparkles size={9} color="white" />
    </div>
    <span class="text-[11.5px] text-muted-foreground truncate">{task.agent}</span>
  </div>
  <div class="w-[140px]">
    {#if showProgress}
      <div class="h-1 rounded-full bg-muted overflow-hidden">
        <div
          class="h-full transition-all"
          style="width: {task.progress}%; background: {progressColor};"
        ></div>
      </div>
    {:else}
      <span class="text-[10.5px] text-muted-foreground/70 font-mono">- -</span>
    {/if}
  </div>
  <div class="w-[120px]">
    <Badge size="sm" variant={cfg.tone}>
      {#snippet icon()}
        {#if task.status === "running"}
          <StatusDot color="hsl(var(--primary))" glow />
        {:else if task.status === "awaiting_approval"}
          <StatusDot color="hsl(var(--warning))" glow />
        {:else if task.status === "failed"}
          <StatusDot color="hsl(var(--destructive))" glow />
        {/if}
      {/snippet}
      {cfg.label}
    </Badge>
  </div>
  <div
    class="w-[90px] text-[10.5px] text-muted-foreground text-right font-mono"
  >
    {task.started ?? task.eta ?? ""}
  </div>
</ListRow>

{#if builder}
  <div
    class="bg-foreground text-[hsl(var(--background))] font-mono text-[10.5px] leading-[1.5] px-4 py-2.5 pl-[182px]"
  >
    {#if task.builderMeta}
      {task.builderMeta}
    {:else}
      task_id={task.id ?? "-"} · agent={task.agent} · status={task.status}<br />
      progress={task.progress ?? 0}% · started={task.started ?? "-"} · eta={task.eta ?? "-"}
    {/if}
  </div>
{/if}
