<script lang="ts">
  import { t } from "svelte-i18n";
  import type { ResolvedApproval } from "$lib/types";
  import { Badge } from "$lib/components/ui/badge";
  import { ClipboardList } from "lucide-svelte";

  interface Props {
    history: ResolvedApproval[];
  }

  let { history }: Props = $props();

  function shortId(id: string): string {
    return id.slice(0, 8);
  }

  function formatWaitDuration(ms: number | null): string {
    if (ms === null || ms === undefined) return "-";
    if (ms < 1_000) return `${ms}ms`;
    const totalSecs = Math.floor(ms / 1_000);
    if (totalSecs < 60) return `${totalSecs}s`;
    const mins = Math.floor(totalSecs / 60);
    const secs = totalSecs % 60;
    if (mins < 60) return `${mins}m ${secs}s`;
    const hours = Math.floor(mins / 60);
    const remMins = mins % 60;
    return `${hours}h ${remMins}m`;
  }

  function formatDate(iso: string | null): string {
    if (!iso) return "-";
    return new Date(iso).toLocaleString();
  }
</script>

{#if history.length === 0}
  <!-- Empty state -->
  <div
    class="flex flex-col items-center justify-center py-12 text-muted-foreground"
    data-testid="approval-history-empty"
  >
    <ClipboardList class="h-8 w-8 mb-2 opacity-50" />
    <p class="text-sm">{$t("approvals.no_history")}</p>
  </div>
{:else}
  <!-- Standard glass-card table -->
  <div class="glass-card glass-border rounded-lg overflow-hidden" data-testid="approval-history-table">
    <table class="w-full text-[13px]">
      <thead class="border-b border-border bg-muted/50">
        <tr>
          <th class="text-left px-3 py-2 text-[11px] text-muted-foreground font-medium">{$t("approvals.table.task")}</th>
          <th class="text-left px-3 py-2 text-[11px] text-muted-foreground font-medium">{$t("approvals.table.agent")}</th>
          <th class="text-left px-3 py-2 text-[11px] text-muted-foreground font-medium">{$t("approvals.table.result")}</th>
          <th class="text-left px-3 py-2 text-[11px] text-muted-foreground font-medium">{$t("approvals.table.wait")}</th>
          <th class="text-left px-3 py-2 text-[11px] text-muted-foreground font-medium">{$t("approvals.table.reason")}</th>
          <th class="text-left px-3 py-2 text-[11px] text-muted-foreground font-medium">{$t("approvals.table.date")}</th>
        </tr>
      </thead>
      <tbody>
        {#each history as item (item.task_id + (item.responded_at ?? ""))}
          <tr class="hover:bg-muted border-b border-border last:border-0">
            <td class="px-3 py-2">
              <code class="text-[11px]">{shortId(item.task_id)}</code>
            </td>
            <td class="px-3 py-2 text-[11px]">{item.agent_name || "-"}</td>
            <td class="px-3 py-2">
              {#if item.approved}
                <Badge variant="success" data-testid="approval-history-badge">
                  {$t("common.approved")}
                </Badge>
              {:else}
                <Badge variant="destructive" data-testid="approval-history-badge">
                  {$t("common.rejected")}
                </Badge>
              {/if}
            </td>
            <td class="px-3 py-2 text-[11px] text-muted-foreground">
              {formatWaitDuration(item.wait_duration_ms)}
            </td>
            <td class="max-w-[200px] truncate px-3 py-2 text-[11px] text-muted-foreground">
              {item.reason ?? "-"}
            </td>
            <td class="px-3 py-2 text-[11px] text-muted-foreground">
              {formatDate(item.responded_at)}
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
{/if}
