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
    /** The provider has no usable OAuth client yet, so connecting cannot work. */
    needsSetup?: boolean;
    selected: boolean;
    onselect: () => void;
  }

  let { connector, accountCount, needsSetup = false, selected, onselect }: Props = $props();

  // Missing credentials outrank the account count: a connector with zero
  // accounts because it is unconfigured is a different state from one the
  // operator simply has not connected yet.
  const railColor = $derived(
    needsSetup
      ? "hsl(var(--warning))"
      : accountCount > 0
        ? "hsl(var(--success))"
        : "hsl(var(--muted-foreground))",
  );

  const subtitle = $derived(
    needsSetup
      ? $t("connections.needs_configuration")
      : accountCount > 0
        ? $t("connections.sidebar_accounts_active", { values: { count: accountCount } })
        : $t("connections.not_connected"),
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
    <div
      class="mt-0.5 truncate text-caption"
      class:text-warning-a11y={needsSetup}
      class:text-muted-foreground={!needsSetup}
    >
      {subtitle}
    </div>
  </div>
</ListRow>
