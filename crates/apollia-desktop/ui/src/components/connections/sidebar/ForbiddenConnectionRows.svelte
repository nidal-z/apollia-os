<script lang="ts">
  /**
   * ForbiddenConnectionRows - the sidebar section listing the connections the
   * organisation policy forbade at the last start. Each keeps its name and
   * account and carries the mark; nothing here can start it again, since only
   * a change of the policy does, at the next start.
   */
  import { t } from "svelte-i18n";
  import { Ban } from "lucide-svelte";
  import { Badge } from "$lib/components/ui/badge";
  import type { Connection } from "$lib/types";

  interface Props {
    /** The connections whose state is `forbidden_by_policy`. */
    connections: Connection[];
  }

  let { connections }: Props = $props();
</script>

{#if connections.length > 0}
  <div class="section-meta mb-1.5 mt-4 px-2 text-overline" data-testid="connections-forbidden-section">
    {$t("connections.sidebar_forbidden_section", { values: { count: connections.length } })}
  </div>
  {#each connections as connection (connection.id)}
    <div
      class="mb-0.5 flex items-center gap-2.5 rounded-md px-2.5 py-2 opacity-80"
      data-testid="connections-forbidden-{connection.id}"
    >
      <Ban size={13} class="shrink-0 text-muted-foreground" aria-hidden="true" />
      <div class="min-w-0 flex-1">
        <div class="truncate text-label-md text-foreground">{connection.label}</div>
        {#if connection.account}
          <div class="mt-0.5 truncate text-caption text-muted-foreground">
            {connection.account}
          </div>
        {/if}
      </div>
      <Badge variant="danger" size="sm" data-testid="connections-forbidden-mark">
        {$t("connections.forbidden_by_policy")}
      </Badge>
    </div>
  {/each}
{/if}
