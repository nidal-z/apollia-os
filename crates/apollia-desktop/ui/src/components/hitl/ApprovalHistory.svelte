<script lang="ts">
  import type { ResolvedApproval } from "$lib/types";
  import { Badge } from "$lib/components/ui/badge";

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
  <p class="text-sm text-muted-foreground">No resolved approvals in the last 7 days.</p>
{:else}
  <div class="overflow-auto rounded border">
    <table class="w-full text-sm">
      <thead>
        <tr class="border-b bg-muted/30 text-left text-xs text-muted-foreground">
          <th class="px-3 py-2">Task</th>
          <th class="px-3 py-2">Agent</th>
          <th class="px-3 py-2">Result</th>
          <th class="px-3 py-2">Wait</th>
          <th class="px-3 py-2">Reason</th>
          <th class="px-3 py-2">Date</th>
        </tr>
      </thead>
      <tbody>
        {#each history as item (item.task_id + (item.responded_at ?? ""))}
          <tr class="border-b last:border-0">
            <td class="px-3 py-2">
              <code class="text-xs">{shortId(item.task_id)}</code>
            </td>
            <td class="px-3 py-2 text-xs">{item.agent_name || "-"}</td>
            <td class="px-3 py-2">
              {#if item.approved}
                <Badge
                  variant="default"
                  class="bg-[var(--apollia-success)] text-white text-[10px]"
                >
                  Approved
                </Badge>
              {:else}
                <Badge variant="destructive" class="text-[10px]">
                  Rejected
                </Badge>
              {/if}
            </td>
            <td class="px-3 py-2 text-xs text-muted-foreground">
              {formatWaitDuration(item.wait_duration_ms)}
            </td>
            <td class="max-w-[200px] truncate px-3 py-2 text-xs text-muted-foreground">
              {item.reason ?? "-"}
            </td>
            <td class="px-3 py-2 text-xs text-muted-foreground">
              {formatDate(item.responded_at)}
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
{/if}
