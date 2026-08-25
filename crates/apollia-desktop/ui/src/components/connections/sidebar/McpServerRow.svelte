<script lang="ts">
  /**
   * McpServerRow - a sidebar row for an installed MCP server.
   *
   * Carries the status rail + dot, a relative sync label, and a right-click
   * context menu (Test / Configure / Disconnect) sharing the row's actions.
   */
  import { t } from "svelte-i18n";
  import { Link as LinkIcon, RefreshCw, Settings, Trash2 } from "lucide-svelte";
  import { ListRow, StatusDot } from "$lib/components/operator";
  import type { ConnectionStatus } from "$lib/connections/status";
  import { ContextMenu, type ActionMenuItem } from "$lib/components/ui/context-menu";
  import ConnectorTile from "../shared/ConnectorTile.svelte";
  import { accentTokenFor } from "$lib/connections/accent";
  import { statusAccent, syncDescriptor } from "$lib/connections/status";
  import type { McpServerStatusView } from "$lib/types";
  import type { McpRowAction } from "../shared/types";

  interface Props {
    server: McpServerStatusView;
    label: string;
    status: ConnectionStatus;
    selected: boolean;
    onselect: () => void;
    oncontext: (action: McpRowAction) => void;
  }

  let { server, label, status, selected, onselect, oncontext }: Props = $props();

  const sync = $derived(syncDescriptor(server.last_call_at));
  const subLabel = $derived.by(() => {
    if (sync) return $t(sync.key, sync.values ? { values: sync.values } : undefined);
    if (status === "error") return $t("connections.status_chip_error");
    if (status === "attention") return $t("connections.status_chip_attention");
    if (status === "active")
      return $t("connections.tools_count_label", { values: { count: server.tools_count } });
    return $t("connections.status_chip_idle");
  });

  const menuItems: ActionMenuItem[] = $derived([
    {
      id: "test",
      label: $t("connections.test"),
      icon: RefreshCw,
      onclick: () => oncontext("test"),
      testid: "mcp-ctx-test",
    },
    {
      id: "configure",
      label: $t("connections.ctx_configure"),
      icon: Settings,
      onclick: () => oncontext("configure"),
      testid: "mcp-ctx-configure",
    },
    {
      id: "disconnect",
      label: $t("integrations.manage.disconnect_button"),
      icon: Trash2,
      variant: "destructive",
      onclick: () => oncontext("disconnect"),
      testid: "mcp-ctx-disconnect",
    },
  ]);
</script>

<ContextMenu items={menuItems} class="mb-0.5" data-testid="mcp-ctx-{server.name}">
  <ListRow
    variant="nav"
    state={selected ? "active" : "default"}
    align="stretch"
    class="text-left"
    onclick={onselect}
    data-testid="sidebar-mcp-{server.name}"
  >
    <div
      class="my-0.5 w-0.5 shrink-0 self-stretch rounded-sm"
      style="background: {statusAccent(status)};"
    ></div>
    <ConnectorTile size="sm" accent={accentTokenFor(server.name)}>
      {#snippet icon()}<LinkIcon size={13} />{/snippet}
    </ConnectorTile>
    <div class="min-w-0 flex-1">
      <div class="flex min-w-0 items-center gap-1.5">
        <span class="truncate text-label-md text-foreground" style:font-weight={selected ? 600 : 500}>
          {label}
        </span>
        {#if status === "active"}
          <StatusDot color="hsl(var(--success))" glow size={5} />
        {:else if status === "attention"}
          <StatusDot color="hsl(var(--warning))" glow size={5} />
        {:else if status === "error"}
          <StatusDot color="hsl(var(--destructive))" glow size={5} />
        {/if}
      </div>
      <div class="mt-0.5 truncate text-caption text-muted-foreground">{subLabel}</div>
    </div>
  </ListRow>
</ContextMenu>
