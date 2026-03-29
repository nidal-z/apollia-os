<script lang="ts">
  import { t } from "svelte-i18n";
  import type { McpServerStatusView } from "$lib/types";
  import ConnectionStatusIndicator from "./ConnectionStatusIndicator.svelte";

  interface Props {
    server: McpServerStatusView;
    onClick: () => void;
  }

  let { server, onClick }: Props = $props();

  function formatUptime(secs: number | null): string {
    if (secs === null) return "—";
    if (secs < 60) return `${secs}s`;
    const mins = Math.floor(secs / 60);
    if (mins < 60) return `${mins}m`;
    const hours = Math.floor(mins / 60);
    if (hours < 24) return `${hours}h`;
    return `${Math.floor(hours / 24)}d`;
  }
</script>

<button
  type="button"
  class="w-full text-left glass-card glass-border rounded-lg px-4 py-3 hover:bg-muted/30 transition-colors flex items-center gap-4"
  onclick={onClick}
  data-testid="builder-server-row"
  data-server-name={server.name}
>
  <ConnectionStatusIndicator connected={server.connected} error={server.error} />

  <span
    class="min-w-0 flex-1 font-mono text-sm font-medium text-foreground truncate"
    data-testid="builder-row-name"
  >
    {server.name}
  </span>

  <span
    class="shrink-0 rounded bg-muted px-1.5 py-0.5 font-mono text-xs text-muted-foreground"
    data-testid="builder-row-transport"
  >
    {server.transport}
  </span>

  <span class="shrink-0 text-xs text-muted-foreground" data-testid="builder-row-tools">
    {$t("integrations.builder.row.tools", { values: { count: server.tools_count } })}
  </span>

  <span class="shrink-0 font-mono text-xs text-muted-foreground" data-testid="builder-row-pid">
    {server.pid !== null
      ? $t("integrations.builder.row.pid", { values: { pid: server.pid } })
      : "—"}
  </span>

  <span class="shrink-0 text-xs text-muted-foreground" data-testid="builder-row-uptime">
    {formatUptime(server.uptime_secs)}
  </span>
</button>
