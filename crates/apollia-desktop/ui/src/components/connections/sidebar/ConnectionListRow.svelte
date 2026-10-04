<script lang="ts">
  /**
   * ConnectionListRow - one row of the single connection list, whatever its kind.
   *
   * Name, what it holds (accounts or tools), its state, and the "Remote" badge
   * when it sends data off this machine. With the developer mode on, a second
   * line gives its technical kind and where it came from. A server row keeps its
   * right-click menu (Test / Configure / Remove).
   */
  import { t } from "svelte-i18n";
  import { Folder, Mail, Plug, RefreshCw, Settings, Trash2, UserRound } from "lucide-svelte";
  import { ListRow, StatusDot } from "$lib/components/operator";
  import { Badge } from "$lib/components/ui/badge";
  import { ContextMenu, type ActionMenuItem } from "$lib/components/ui/context-menu";
  import { developerMode } from "$lib/stores/developerMode";
  import { accentTokenFor } from "$lib/connections/accent";
  import { provenanceLine, type ConnectionRow } from "$lib/connections/connectionRows";
  import { statusAccent } from "$lib/connections/status";
  import ConnectorTile from "../shared/ConnectorTile.svelte";
  import type { McpRowAction } from "../shared/types";

  interface Props {
    row: ConnectionRow;
    selected: boolean;
    onselect: () => void;
    oncontext?: (action: McpRowAction) => void;
  }

  let { row, selected, onselect, oncontext }: Props = $props();

  const mcpName = $derived(row.selection.kind === "mcp" ? row.selection.name : null);
  const provider = $derived(row.selection.kind === "native" ? row.selection.provider : null);

  const Icon = $derived(
    row.kind === "native" ? UserRound : row.kind === "imap" ? Mail : row.kind === "folder" ? Folder : Plug,
  );

  const subLabel = $derived.by(() => {
    if (row.status === "attention") return $t("connections.status_chip_attention");
    if (row.status === "error") return $t("connections.status_chip_error");
    if (row.accounts.length > 1)
      return $t("connections.sidebar_accounts_active", { values: { count: row.accounts.length } });
    if (row.accounts.length === 1) return row.accounts[0];
    if (row.status === "idle") return $t("connections.status_chip_idle");
    return $t("connections.tools_count_label", { values: { count: row.toolCount } });
  });

  const menuItems: ActionMenuItem[] = $derived([
    {
      id: "test",
      label: $t("connections.test"),
      icon: RefreshCw,
      onclick: () => oncontext?.("test"),
      testid: "mcp-ctx-test",
    },
    {
      id: "configure",
      label: $t("connections.ctx_configure"),
      icon: Settings,
      onclick: () => oncontext?.("configure"),
      testid: "mcp-ctx-configure",
    },
    {
      id: "disconnect",
      label: $t("integrations.manage.disconnect_button"),
      icon: Trash2,
      variant: "destructive",
      onclick: () => oncontext?.("disconnect"),
      testid: "mcp-ctx-disconnect",
    },
  ]);
</script>

{#snippet body()}
  <ListRow
    variant="nav"
    state={selected ? "active" : "default"}
    align="stretch"
    class="text-left"
    onclick={onselect}
    data-testid={mcpName !== null
      ? `sidebar-mcp-${mcpName}`
      : provider !== null
        ? `sidebar-native-${provider}`
        : `sidebar-connection-${row.key}`}
  >
    <div
      class="my-0.5 w-0.5 shrink-0 self-stretch rounded-sm"
      style="background: {statusAccent(row.status)};"
    ></div>
    <ConnectorTile
      size="sm"
      accent={row.kind === "native" ? "hsl(var(--primary))" : accentTokenFor(row.label)}
    >
      {#snippet icon()}<Icon size={13} />{/snippet}
    </ConnectorTile>
    <div class="min-w-0 flex-1">
      <div class="flex min-w-0 items-center gap-1.5">
        <span class="truncate text-label-md text-foreground" style:font-weight={selected ? 600 : 500}>
          {row.label}
        </span>
        {#if row.status === "active"}
          <StatusDot color="hsl(var(--success))" glow size={5} />
        {:else if row.status === "attention"}
          <StatusDot color="hsl(var(--warning))" glow size={5} />
        {:else if row.status === "error"}
          <StatusDot color="hsl(var(--destructive))" glow size={5} />
        {/if}
        {#if row.remote}
          <Badge size="sm" variant="info" data-testid="connection-remote-{row.key}">
            {$t("connections.remote")}
          </Badge>
        {/if}
      </div>
      <div class="mt-0.5 truncate text-caption text-muted-foreground">{subLabel}</div>
      {#if $developerMode}
        <div
          class="mt-0.5 truncate font-mono text-caption text-muted-foreground/80"
          data-testid="connection-provenance-{row.key}"
        >
          {provenanceLine(row, $t("connections.badge_official"))}
        </div>
      {/if}
    </div>
  </ListRow>
{/snippet}

{#if mcpName !== null && oncontext}
  <ContextMenu items={menuItems} class="mb-0.5" data-testid="mcp-ctx-{mcpName}">
    {@render body()}
  </ContextMenu>
{:else}
  <div class="mb-0.5">{@render body()}</div>
{/if}
