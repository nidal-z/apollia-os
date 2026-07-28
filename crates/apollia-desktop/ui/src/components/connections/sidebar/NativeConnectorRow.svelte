<script lang="ts">
  /**
   * NativeConnectorRow - a sidebar row for a native Apollia connector.
   */
  import { t } from "svelte-i18n";
  import { Link as LinkIcon } from "lucide-svelte";
  import { ListRow } from "$lib/components/operator";
  import ConnectorTile from "../shared/ConnectorTile.svelte";
  import type { NativeConnectorCard } from "../shared/types";

  interface Props {
    connector: NativeConnectorCard;
    accountCount: number;
    selected: boolean;
    onselect: () => void;
  }

  let { connector, accountCount, selected, onselect }: Props = $props();

  const railColor = $derived(
    accountCount > 0 ? "hsl(var(--success))" : "hsl(var(--muted-foreground))",
  );
</script>

<ListRow
  variant="nav"
  state={selected ? "active" : "default"}
  align="stretch"
  class="mb-0.5 text-left"
  onclick={onselect}
  data-testid="sidebar-native-{connector.id}"
>
  <div class="my-0.5 w-0.5 shrink-0 self-stretch rounded-sm" style="background: {railColor};"></div>
  <ConnectorTile size="sm" accent="hsl(var(--primary))">
    {#snippet icon()}<LinkIcon size={13} />{/snippet}
  </ConnectorTile>
  <div class="min-w-0 flex-1">
    <div class="truncate text-label-md text-foreground" style:font-weight={selected ? 600 : 500}>
      {connector.name}
    </div>
    <div class="mt-0.5 truncate text-caption text-muted-foreground">
      {accountCount > 0
        ? $t("connections.sidebar_accounts_active", { values: { count: accountCount } })
        : $t("connections.not_connected")}
    </div>
  </div>
</ListRow>
