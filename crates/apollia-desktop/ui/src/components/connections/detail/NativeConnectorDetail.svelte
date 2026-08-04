<script lang="ts">
  /**
   * NativeConnectorDetail - detail pane for a native Apollia connector.
   *
   * DetailHeader (identity, status badge, add-account CTA) over a TabBar with
   * Accounts / Capabilities / Settings. Tab is orchestrator-bound so a context
   * action can deep-link to a tab. The OAuth flow itself lives in the parent.
   */
  import { t } from "svelte-i18n";
  import { Plus, Settings2, Link as LinkIcon } from "lucide-svelte";
  import { DetailHeader } from "$lib/components/operator";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import { Card } from "$lib/components/ui/card";
  import { TabBar } from "$lib/components/ui/tabs";
  import ConnectorTile from "../shared/ConnectorTile.svelte";
  import StatusBadge from "../shared/StatusBadge.svelte";
  import NativeAccountsTab from "./NativeAccountsTab.svelte";
  import CapabilityMatrix from "./CapabilityMatrix.svelte";
  import { scopeLabelKeys, providerEndpoint } from "$lib/connections/capabilities";
  import type { NativeConnectorCard, NativeTab } from "../shared/types";
  import type { OauthAccountInfo } from "$lib/ipc/connections";

  interface Props {
    connector: NativeConnectorCard;
    accounts: OauthAccountInfo[];
    loading: boolean;
    error: string | null;
    /** The provider has no usable OAuth client yet, so connecting cannot work. */
    needsSetup?: boolean;
    tab: NativeTab;
    onconnect: () => void;
    onconfigure?: () => void;
    ondisconnect: (account: OauthAccountInfo) => void;
    ontabchange: (tab: NativeTab) => void;
  }

  let {
    connector,
    accounts,
    loading,
    error,
    needsSetup = false,
    tab,
    onconnect,
    onconfigure,
    ondisconnect,
    ontabchange,
  }: Props = $props();
</script>

<DetailHeader title={connector.name} meta={connector.description}>
  {#snippet leading()}
    <ConnectorTile size="lg" accent="hsl(var(--primary))" glow>
      {#snippet icon()}<LinkIcon size={18} />{/snippet}
    </ConnectorTile>
  {/snippet}
  {#snippet badges()}
    {#if needsSetup}
      <StatusBadge
        status="attention"
        label={$t("connections.needs_configuration")}
        data-testid="native-needs-config-{connector.id}"
      />
    {:else if accounts.length > 0}
      <StatusBadge
        status="active"
        label={$t("connections.badge_active_count", { values: { count: accounts.length } })}
      />
    {:else}
      <StatusBadge status="idle" label={$t("connections.not_connected")} />
    {/if}
    <Badge variant="primary" size="sm">{$t("connections.native_section_tag")}</Badge>
  {/snippet}
  {#snippet actions()}
    {#if needsSetup}
      <Button
        variant="primary-solid"
        size="sm"
        onclick={() => onconfigure?.()}
        disabled={loading}
        data-testid="oauth-configure-{connector.id}"
      >
        {#snippet icon()}<Settings2 size={12} />{/snippet}
        {$t("connections.configure_credentials")}
      </Button>
    {:else}
      <Button
        variant="primary-solid"
        size="sm"
        onclick={onconnect}
        disabled={loading}
        data-testid="oauth-add-account-{connector.id}"
      >
        {#snippet icon()}<Plus size={12} />{/snippet}
        {accounts.length > 0 ? $t("connections.add_account") : $t("connections.connect")}
      </Button>
    {/if}
  {/snippet}
</DetailHeader>

<div class="px-8 pt-3.5">
  <TabBar
    variant="underline"
    testidPrefix="native-detail"
    activeTab={tab}
    items={[
      { key: "accounts", label: $t("connections.native_tab_accounts"), count: accounts.length },
      { key: "capabilities", label: $t("connections.native_tab_capabilities") },
      { key: "settings", label: $t("connections.native_tab_settings") },
    ]}
    ontabchange={(k) => ontabchange(k as NativeTab)}
  />
</div>

<div class="flex-1 overflow-auto px-8 py-5">
  {#if tab === "accounts"}
    <NativeAccountsTab
      {connector}
      {accounts}
      {loading}
      {error}
      {needsSetup}
      {onconnect}
      {onconfigure}
      {ondisconnect}
    />
  {:else if tab === "capabilities"}
    <CapabilityMatrix providerId={connector.id} />
  {:else}
    <div class="max-w-2xl space-y-4">
      <Card class="px-4 py-3.5">
        <div class="mb-2 text-overline text-muted-foreground/60">
          {$t("connections.settings_permissions_title")}
        </div>
        <ul class="space-y-1 text-body-xs text-foreground/85">
          {#each scopeLabelKeys(connector.id) as scopeKey (scopeKey)}
            <li class="flex items-start gap-2">
              <span class="mt-1.5 h-1 w-1 shrink-0 rounded-full bg-muted-foreground/60"></span>
              <span>{$t(scopeKey)}</span>
            </li>
          {/each}
        </ul>
      </Card>
      <Card class="px-4 py-3.5">
        <div class="mb-2 text-overline text-muted-foreground/60">
          {$t("connections.settings_provider_title")}
        </div>
        <p class="text-body-xs text-foreground/85">{providerEndpoint(connector.id)}</p>
        <p class="mt-1.5 text-caption text-muted-foreground">
          {$t("connections.settings_token_rotation")}
        </p>
      </Card>
    </div>
  {/if}
</div>
