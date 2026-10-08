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
  import { ChevronRight, Folder, Mail, Plug, RefreshCw, Settings, Trash2, UserRound } from "lucide-svelte";
  import { ListRow } from "$lib/components/operator";
  import { Badge } from "$lib/components/ui/badge";
  import { ContextMenu, type ActionMenuItem } from "$lib/components/ui/context-menu";
  import { developerMode } from "$lib/stores/developerMode";
  import { provenanceLine, type ConnectionRow } from "$lib/connections/connectionRows";
  import { statusBadgeVariant, statusLabelKey } from "$lib/connections/status";
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
    align="center"
    class="gap-3 !rounded-none px-4 py-3 text-left"
    onclick={onselect}
    data-testid={mcpName !== null
      ? `sidebar-mcp-${mcpName}`
      : provider !== null
        ? `sidebar-native-${provider}`
        : `sidebar-connection-${row.key}`}
  >
    <span class="inline-flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-muted text-foreground">
      <Icon size={16} strokeWidth={1.75} />
    </span>
    <div class="min-w-0 flex-1">
      <div class="flex min-w-0 items-center gap-2">
        <span class="truncate text-body-sm font-medium text-foreground">{row.label}</span>
        {#if row.remote}
          <Badge size="sm" variant="info" data-testid="connection-remote-{row.key}">
            {$t("connections.remote")}
          </Badge>
        {/if}
      </div>
      <div class="mt-0.5 truncate text-caption text-muted-foreground">
        {subLabel}{#if $developerMode}<span
            class="text-muted-foreground/70"
            data-testid="connection-provenance-{row.key}"
          >
            · {provenanceLine(row, $t("connections.badge_official"))}</span
          >{/if}
      </div>
    </div>
    <Badge size="sm" variant={statusBadgeVariant(row.status)}>{$t(statusLabelKey(row.status))}</Badge>
    <ChevronRight size={15} class="shrink-0 text-muted-foreground" aria-hidden="true" />
  </ListRow>
{/snippet}

{#if mcpName !== null && oncontext}
  <ContextMenu items={menuItems} data-testid="mcp-ctx-{mcpName}">
    {@render body()}
  </ContextMenu>
{:else}
  <div>{@render body()}</div>
{/if}
