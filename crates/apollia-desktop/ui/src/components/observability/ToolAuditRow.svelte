<script lang="ts">
  // Audit row for a single tool invocation, with an expandable detail panel.
  // Extracted verbatim from AuditTrailTable so the table stays focused on the
  // row dispatch. Expansion state is owned by the parent; this row reports
  // clicks through `ontoggle`.
  import { t } from "svelte-i18n";
  import { CheckCircle2, XCircle, ChevronDown } from "lucide-svelte";
  import type { AuditTrailEntry } from "$lib/types";

  interface Props {
    entry: AuditTrailEntry;
    expanded: boolean;
    ontoggle: (id: string) => void;
  }

  let { entry, expanded, ontoggle }: Props = $props();

  type EntryStatus = "ok" | "error" | "unknown";

  /** Status of an audit entry, derived from exit_code + stderr presence.
   *  An MCP tool with no exit_code is considered "ok" unless stderr is set. */
  const status = $derived<EntryStatus>(
    entry.exit_code !== null && entry.exit_code !== undefined
      ? entry.exit_code === 0
        ? "ok"
        : "error"
      : entry.stderr && entry.stderr.trim().length > 0
        ? "error"
        : entry.duration_ms !== null && entry.duration_ms !== undefined
          ? "ok"
          : "unknown",
  );

  function formatTimestamp(iso: string): string {
    if (!iso) return "";
    return new Date(iso).toLocaleString();
  }

  function formatDuration(ms: number | null): string {
    if (ms === null || ms === undefined) return "-";
    if (ms < 1000) return `${ms}ms`;
    return `${(ms / 1000).toFixed(1)}s`;
  }
</script>

<tr
  class="cursor-pointer border-b border-border/30 last:border-0 transition-colors hover:bg-muted/40"
  class:bg-muted={expanded}
  onclick={() => ontoggle(entry.id)}
  data-testid="audit-row-{entry.id}"
>
  <td class="px-5 py-2.5 text-[11.5px] text-muted-foreground tabular-nums whitespace-nowrap">
    {formatTimestamp(entry.timestamp)}
  </td>
  <td class="px-3 py-2.5">
    <code class="font-mono text-[12px] text-foreground">{entry.tool_name}</code>
  </td>
  <td class="px-3 py-2.5 text-muted-foreground">{entry.agent_name}</td>
  <td class="px-3 py-2.5 text-right tabular-nums">{formatDuration(entry.duration_ms)}</td>
  <td class="px-3 py-2.5">
    {#if status === "ok"}
      <span class="inline-flex items-center gap-1.5 text-success">
        <CheckCircle2 class="h-3.5 w-3.5" />
        <span class="text-[11.5px] font-medium">{$t('observability.audit_status_ok')}</span>
      </span>
    {:else if status === "error"}
      <span class="inline-flex items-center gap-1.5 text-destructive">
        <XCircle class="h-3.5 w-3.5" />
        <span class="text-[11.5px] font-medium">{$t('observability.audit_status_error')}</span>
      </span>
    {:else}
      <span class="text-[11.5px] text-muted-foreground/60">
        {$t('observability.audit_status_unknown')}
      </span>
    {/if}
  </td>
  <td class="px-5 py-2.5 text-right">
    <ChevronDown
      class="h-3.5 w-3.5 text-muted-foreground/60 transition-transform inline-block"
      style={expanded ? "transform: rotate(180deg);" : ""}
    />
  </td>
</tr>

{#if expanded}
  <tr class="bg-muted/20">
    <td colspan="6" class="px-5 pb-4 pt-1">
      <div class="space-y-3 rounded-lg glass-inset border border-border/30 p-4">
        {#if entry.args_json}
          <div>
            <span class="section-meta text-[10px] tracking-[1.4px]">
              {$t('observability.table.arguments')}
            </span>
            <pre
              class="mt-1.5 overflow-x-auto rounded glass-surface p-3 text-[11.5px] font-mono leading-relaxed"
            >{entry.args_json}</pre>
          </div>
        {/if}
        {#if entry.stdout}
          <div>
            <span class="section-meta text-[10px] tracking-[1.4px]">
              stdout
            </span>
            <pre
              class="mt-1.5 overflow-x-auto rounded glass-surface p-3 text-[11.5px] font-mono leading-relaxed"
            >{entry.stdout}</pre>
          </div>
        {/if}
        {#if entry.stderr}
          <div>
            <span class="section-meta text-[10px] tracking-[1.4px] text-destructive">
              stderr
            </span>
            <pre
              class="mt-1.5 overflow-x-auto rounded glass-surface p-3 text-[11.5px] font-mono leading-relaxed text-destructive"
            >{entry.stderr}</pre>
          </div>
        {/if}
        {#if !entry.args_json && !entry.stdout && !entry.stderr}
          <p class="text-[12px] text-muted-foreground italic">
            {$t('observability.table.no_details')}
          </p>
        {/if}
      </div>
    </td>
  </tr>
{/if}
