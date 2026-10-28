<script lang="ts">
  /**
   * ConnectionListItem - one connection of the list, whatever its kind: its
   * kind as an icon (the logo of its service when it has one), its name, the account it acts as or what it brings, the
   * package that brought it, and its state on the right, with Check when it
   * was never checked. With the developer
   * mode on, the second line also gives its technical kind and origin.
   */
  import { t } from "svelte-i18n";
  import { Folder, Mail, Plug, UserRound } from "lucide-svelte";
  import { ListRow } from "$lib/components/operator";
  import { Badge } from "$lib/components/ui/badge";
  import { developerMode } from "$lib/stores/developerMode";
  import { KIND_LABEL_KEYS, provenanceLine, serverNameOf, type ConnectionRow } from "$lib/connections/connectionRows";
  import { brandLogoPath } from "$lib/connections/brandLogo";
  import ConnectionStateBadge from "../shared/ConnectionStateBadge.svelte";
  import BrandLogo from "../shared/BrandLogo.svelte";

  interface Props {
    row: ConnectionRow;
    selected: boolean;
    onselect: () => void;
    /** Check the connection now, from its badge. */
    oncheck?: () => void;
    /** A check of this connection is under way. */
    checking?: boolean;
  }

  let { row, selected, onselect, oncheck, checking = false }: Props = $props();

  /** A service is anchored by its server name, any other connection by its identifier. */
  const serverName = $derived(serverNameOf(row.key));

  const Icon = $derived(
    row.kind === "native" ? UserRound : row.kind === "imap" ? Mail : row.kind === "folder" ? Folder : Plug,
  );

  const logo = $derived(brandLogoPath(row.icon));

  const subLabel = $derived(
    row.account ?? $t("connections.tools_count_label", { values: { count: row.toolCount } }),
  );
</script>

<ListRow
  variant="nav"
  state={selected ? "active" : "default"}
  align="center"
  class="gap-3 px-3 py-2.5 text-left"
  onclick={onselect}
  data-testid={serverName !== null ? `sidebar-mcp-${serverName}` : `sidebar-connection-${row.key}`}
>
  {#if logo}
    <BrandLogo path={logo} size="list" data-testid="connection-logo-{row.key}" />
  {:else}
    <span
      class="inline-flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-muted text-foreground"
      title={$t(KIND_LABEL_KEYS[row.kind])}
    >
      <Icon size={15} strokeWidth={1.75} aria-hidden="true" />
    </span>
  {/if}
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
      {subLabel}{#if $developerMode}<span class="text-muted-foreground/70" data-testid="connection-provenance-{row.key}">
          · {provenanceLine(row, $t("connections.badge_official"))}</span
        >{/if}
    </div>
    {#if row.package}
      <div class="mt-0.5 truncate text-caption text-muted-foreground" data-testid="connection-package-{row.key}">
        {$t("connections.list.brought_by", { values: { package: row.package } })}
      </div>
    {/if}
  </div>
  <ConnectionStateBadge
    status={row.status}
    reason={row.reason}
    checkedAt={row.checkedAt}
    {oncheck}
    {checking}
    data-testid="connection-state-{row.key}"
  />
</ListRow>
