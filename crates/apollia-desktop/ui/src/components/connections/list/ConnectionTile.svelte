<script lang="ts">
  /**
   * ConnectionTile - one connection of the dashboard, whatever its kind: its
   * logo (or the icon of its kind), its name, the account or the folder it
   * acts on, whether it is reached online or runs on this computer, its state
   * coloured and dated, what it brings in words and its
   * number of tools, the agents that use it and when it was last used, the
   * package that brought it, and the one action that repairs or checks it.
   * With the developer mode on, it also gives its technical kind and origin.
   * Choosing the tile opens its card.
   */
  import { t } from "svelte-i18n";
  import { Bot, Cloud, Folder, Mail, Monitor, UserRound } from "lucide-svelte";
  import { ListRow } from "$lib/components/operator";
  import { Button } from "$lib/components/ui/button";
  import { developerMode } from "$lib/stores/developerMode";
  import { APOLLIA_AGENT_ID } from "$lib/chat/chatTargets";
  import { KIND_LABEL_KEYS, provenanceLine, serverNameOf, type ConnectionRow } from "$lib/connections/connectionRows";
  import { brandLogoPath } from "$lib/connections/brandLogo";
  import { repairAction, type RepairKind } from "$lib/connections/connectionCard";
  import { usedDescriptor } from "$lib/connections/connectionSections";
  import { bringsKeys, orderedAgents, type TileFacts } from "$lib/connections/connectionDashboard";
  import ConnectionStateBadge from "../shared/ConnectionStateBadge.svelte";
  import BrandLogo from "../shared/BrandLogo.svelte";

  interface Props {
    row: ConnectionRow;
    /** Its agents and last use, once read; absent while unread or past the reading bound. */
    facts?: TileFacts;
    selected: boolean;
    onselect: () => void;
    /** Check the connection now, from its badge. */
    oncheck?: () => void;
    /** A check of this connection is under way. */
    checking?: boolean;
    /** The repair the tile offers, by its kind. */
    onrepair?: (kind: RepairKind) => void;
  }

  let { row, facts, selected, onselect, oncheck, checking = false, onrepair }: Props = $props();

  /** A service is anchored by its server name, any other connection by its identifier. */
  const serverName = $derived(serverNameOf(row.key));

  /** The icon of a kind with no logo; a service with none shows its initials. */
  const Icon = $derived(
    row.kind === "native" ? UserRound : row.kind === "imap" ? Mail : row.kind === "folder" ? Folder : null,
  );

  const logo = $derived(brandLogoPath(row.icon));
  const where = $derived(row.account ?? row.path);
  const repair = $derived(repairAction(row));
  const brings = $derived(
    [
      ...bringsKeys(row).map((key) => $t(key)),
      $t("connections.tools_count_label", { values: { count: row.toolCount } }),
    ].join(" · "),
  );
  const agentNames = $derived(
    orderedAgents(facts?.agents ?? [])
      .map((id) => (id === APOLLIA_AGENT_ID ? $t("authorizations.apollia_name") : id))
      .join(", "),
  );
  const lastUsed = $derived(usedDescriptor(facts?.lastUsedAt ?? null, Date.now()));
  const use = $derived(
    facts
      ? [
          $t("connections.tile.agents", { values: { count: facts.agents.length } }),
          lastUsed ? $t(lastUsed.key, { values: lastUsed.values }) : null,
        ]
          .filter((part): part is string => part !== null)
          .join(" · ")
      : "",
  );

  /** Act without choosing the tile the button sits on. */
  function act(event: Event, kind: RepairKind): void {
    event.stopPropagation();
    onrepair?.(kind);
  }
</script>

{#snippet kindIcon()}
  {#if Icon}<Icon size={16} strokeWidth={1.75} aria-hidden="true" />{/if}
{/snippet}

<ListRow
  variant="nav"
  state={selected ? "active" : "default"}
  align="stretch"
  class="h-full flex-col gap-3 rounded-xl border p-4 text-left shadow-sm transition-[border-color,box-shadow,background-color] {selected
    ? 'border-primary/50 bg-primary/5 ring-1 ring-primary/30'
    : 'border-border bg-card hover:border-primary/30 hover:bg-card hover:shadow-md'}"
  onclick={onselect}
  aria-label={$t("connections.tile.open", { values: { name: row.label } })}
  data-testid={serverName !== null ? `sidebar-mcp-${serverName}` : `sidebar-connection-${row.key}`}
>
  <div class="flex min-w-0 items-start gap-3">
    {#if logo}
      <BrandLogo path={logo} name={row.label} data-testid="connection-logo-{row.key}" />
    {:else}
      <span class="contents" title={$t(KIND_LABEL_KEYS[row.kind])} data-testid="connection-kind-icon-{row.key}">
        <BrandLogo name={row.label} icon={Icon ? kindIcon : undefined} />
      </span>
    {/if}
    <div class="min-w-0 flex-1">
      <span class="block truncate text-body-sm font-semibold text-foreground" data-testid="connection-tile-name-{row.key}">
        {row.label}
      </span>
      {#if where}
        <div class="mt-0.5 truncate text-caption text-muted-foreground" title={where} data-testid="connection-tile-where-{row.key}">
          {where}
        </div>
      {/if}
      {#if $developerMode}
        <div class="mt-0.5 truncate text-caption text-muted-foreground/70" data-testid="connection-provenance-{row.key}">
          {provenanceLine(row, $t("connections.badge_official"))}
        </div>
      {/if}
    </div>
    <span
      class="inline-flex shrink-0 items-center gap-1 text-caption text-muted-foreground"
      data-testid={row.remote ? `connection-remote-${row.key}` : `connection-machine-${row.key}`}
    >
      {#if row.remote}<Cloud size={12} aria-hidden="true" />{:else}<Monitor size={12} aria-hidden="true" />{/if}
      {row.remote ? $t("connections.remote") : $t("connections.machine")}
    </span>
  </div>

  <ConnectionStateBadge
    status={row.status}
    reason={row.reason}
    checkedAt={row.checkedAt}
    inline
    data-testid="connection-state-{row.key}"
  />

  <div class="truncate text-caption text-foreground/85" data-testid="connection-tile-brings-{row.key}">{brings}</div>

  {#if facts || row.package}
    <div class="flex min-w-0 flex-col gap-1 border-t border-border/60 pt-2.5">
      {#if facts}
        <div class="flex min-w-0 items-center gap-1.5 text-caption text-muted-foreground" data-testid="connection-tile-use-{row.key}">
          <Bot size={12} class="shrink-0" aria-hidden="true" />
          <span class="truncate" title={agentNames || undefined}>{use}</span>
        </div>
      {/if}
      {#if row.package}
        <div class="truncate text-caption text-muted-foreground" data-testid="connection-package-{row.key}">
          {$t("connections.list.brought_by", { values: { package: row.package } })}
        </div>
      {/if}
    </div>
  {/if}

  {#if repair && onrepair}
    <div class="mt-auto pt-0.5">
      <Button
        variant="outline"
        size="sm"
        onclick={(event: MouseEvent) => act(event, repair.kind)}
        onkeydown={(event: KeyboardEvent) => event.stopPropagation()}
        data-testid="connection-tile-repair-{row.key}"
        data-repair={repair.kind}
      >
        {$t(repair.labelKey)}
      </Button>
    </div>
  {:else if oncheck && row.status === "not_checked"}
    <!-- A connection nothing has tried yet: its one action, Check, sits in
         the foot like every other tile's action. -->
    <div class="mt-auto pt-0.5">
      <Button
        variant="outline"
        size="sm"
        loading={checking}
        onclick={(event: MouseEvent) => {
          event.stopPropagation();
          oncheck?.();
        }}
        onkeydown={(event: KeyboardEvent) => event.stopPropagation()}
        data-testid="connection-state-{row.key}-check"
      >
        {$t("connections.state.action.check")}
      </Button>
    </div>
  {/if}
</ListRow>
