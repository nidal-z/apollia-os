<script lang="ts">
  /**
   * AgentRunRow - one execution of the agent: its status, a preview, and
   * when it ran. Opened, it shows its input, its status, its output and its
   * steps; in developer mode, the technical detail of the run too.
   */
  import { t, locale } from "svelte-i18n";
  import { ChevronDown } from "lucide-svelte";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import { DeveloperOnly } from "$lib/components/shared";
  import { formatDuration, formatRelativeTime } from "$lib/utils";
  import type { TaskSummary } from "$lib/types";
  import ExecutionTrace from "../../observability/ExecutionTrace.svelte";
  import SmartOutput from "../../common/SmartOutput.svelte";
  import { statusMeta } from "../../tasks/taskStatus";

  interface Props {
    run: TaskSummary;
    open: boolean;
    ontoggle: () => void;
  }

  let { run, open, ontoggle }: Props = $props();

  const meta = $derived(statusMeta(run.status));
  const running = $derived(run.status === "working" || run.status === "submitted");
  const failed = $derived(run.status === "failed");
  const labelClass = "text-caption font-semibold text-muted-foreground";
</script>

<li class="rounded-md border border-border/60" data-testid="agent-sheet-run" data-run-id={run.id} data-open={open}>
  <div class="flex items-center gap-3 px-3 py-2">
    <Badge size="sm" variant={meta.variant} class="shrink-0">{$t(meta.i18nKey)}</Badge>
    <span class="min-w-0 flex-1 truncate text-body-xs text-foreground/85">
      {run.input_preview || $t("common.no_input")}
    </span>
    <span class="shrink-0 text-caption text-muted-foreground">
      {formatRelativeTime(run.created_at, $locale ?? "fr")}
    </span>
    <Button
      variant="ghost"
      size="sm"
      onclick={ontoggle}
      aria-expanded={open}
      data-testid="agent-sheet-run-open"
    >
      {#snippet icon()}<ChevronDown size={12} class={open ? "rotate-180" : ""} />{/snippet}
      {open ? $t("agent_sheet.run_close") : $t("agent_sheet.run_open")}
    </Button>
  </div>

  {#if open}
    <div class="space-y-3 border-t border-border/60 px-3 py-3" data-testid="agent-sheet-run-detail">
      <div data-testid="agent-sheet-run-input">
        <div class={labelClass}>{$t("agent_sheet.run_input")}</div>
        <p class="m-0 mt-1 whitespace-pre-wrap text-body-xs text-foreground/85">
          {run.input_preview || $t("common.no_input")}
        </p>
      </div>
      <div data-testid="agent-sheet-run-status">
        <div class={labelClass}>{$t("agent_sheet.run_status")}</div>
        <p class="m-0 mt-1 text-body-xs text-foreground/85">
          {$t(meta.i18nKey)}{#if run.duration_ms !== undefined}
            · {formatDuration(run.duration_ms)}{/if}
        </p>
      </div>
      <div data-testid="agent-sheet-run-output">
        <div class={labelClass}>{failed ? $t("agent_sheet.run_error") : $t("agent_sheet.run_output")}</div>
        {#if run.output_text}
          {#if failed}
            <p class="m-0 mt-1 whitespace-pre-wrap break-all text-body-xs text-destructive">{run.output_text}</p>
          {:else}
            <SmartOutput output={run.output_text} class="mt-1" />
          {/if}
        {:else}
          <p class="m-0 mt-1 text-body-xs italic text-muted-foreground">
            {running ? $t("agent_sheet.run_still_working") : $t("agent_sheet.run_no_output")}
          </p>
        {/if}
      </div>
      <div data-testid="agent-sheet-run-trace">
        <div class={labelClass}>{$t("agent_sheet.run_steps")}</div>
        <ExecutionTrace taskId={run.id} context="task" mode={running ? "live" : "replay"} />
      </div>
      <DeveloperOnly>
        <dl class="m-0 grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 rounded-md bg-muted/40 px-3 py-2 text-caption" data-testid="agent-sheet-run-technical">
          <dt class="text-muted-foreground">{$t("agent_sheet.run_tech_id")}</dt>
          <dd class="m-0 font-mono text-foreground/85">{run.id}</dd>
          <dt class="text-muted-foreground">{$t("agent_sheet.run_tech_agent")}</dt>
          <dd class="m-0 font-mono text-foreground/85">{run.agent_id}</dd>
          <dt class="text-muted-foreground">{$t("agent_sheet.run_tech_status")}</dt>
          <dd class="m-0 font-mono text-foreground/85">{run.status}</dd>
          <dt class="text-muted-foreground">{$t("agent_sheet.run_tech_started")}</dt>
          <dd class="m-0 font-mono text-foreground/85">{run.created_at}</dd>
        </dl>
      </DeveloperOnly>
    </div>
  {/if}
</li>
