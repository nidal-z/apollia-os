<script lang="ts">
  /**
   * NativeAccountsTab - connected accounts list for a native connector, each
   * Google account with the Drive folder its agents work in, each Microsoft
   * account with the OneDrive folders and files Apollia may read.
   */
  import { t } from "svelte-i18n";
  import { Plus, Settings2 } from "lucide-svelte";
  import { ErrorBanner } from "$lib/components/operator";
  import { Banner } from "$lib/components/ui/banner";
  import { Card } from "$lib/components/ui/card";
  import { Button } from "$lib/components/ui/button";
  import type { NativeConnectorCard } from "../shared/types";
  import type { OauthAccountInfo } from "$lib/ipc/connections";
  import type { DriveFolderStatus } from "$lib/ipc/oauthClients";
  import { driveFolderFor, listDriveFolders } from "$lib/connections/oauthClient";
  import AccountDriveFolder from "./AccountDriveFolder.svelte";
  import AccountOneDriveChoices from "./AccountOneDriveChoices.svelte";

  interface Props {
    connector: NativeConnectorCard;
    accounts: OauthAccountInfo[];
    loading: boolean;
    error: string | null;
    /** The provider has no usable OAuth client yet, so connecting cannot work. */
    needsSetup?: boolean;
    onconnect: () => void;
    onconfigure?: () => void;
    ondisconnect: (account: OauthAccountInfo) => void;
  }

  let {
    connector,
    accounts,
    loading,
    error,
    needsSetup = false,
    onconnect,
    onconfigure,
    ondisconnect,
  }: Props = $props();

  // The Drive folder of each Google account, read again whenever the account
  // list changes.
  let driveFolders = $state<DriveFolderStatus[]>([]);
  const showsDriveFolders = $derived(connector.id === "google" && accounts.length > 0);
  const showsOneDriveChoices = $derived(connector.id === "microsoft");

  async function refreshDriveFolders(): Promise<void> {
    try {
      driveFolders = await listDriveFolders();
    } catch {
      driveFolders = [];
    }
  }

  const accountKey = $derived(accounts.map((a) => a.account_id).join("\n"));

  $effect(() => {
    if (showsDriveFolders && accountKey) void refreshDriveFolders();
  });
</script>

{#if error}
  <ErrorBanner message={error} class="mb-3" />
{/if}

{#if needsSetup}
  <!-- Shown instead of the connect prompt: no Apollia build ships a client, so
       offering Connect here would send the operator to a consent screen that
       cannot complete. -->
  <Banner
    variant="warning"
    title={$t("connections.needs_client_title")}
    class="mb-3 max-w-2xl"
    data-testid="native-needs-config-banner-{connector.id}"
  >
    {$t("connections.needs_client_body", { values: { provider: connector.name } })}
    <div class="mt-3">
      <Button
        variant="primary-solid"
        size="sm"
        onclick={() => onconfigure?.()}
        data-testid="oauth-configure-banner-{connector.id}"
      >
        {#snippet icon()}<Settings2 size={12} />{/snippet}
        {$t("connections.configure_credentials")}
      </Button>
    </div>
  </Banner>
{:else if accounts.length === 0}
  <Card class="max-w-2xl p-6 text-center">
    <p class="mb-3 text-body-sm text-muted-foreground">
      {$t("connections.no_account_for", { values: { name: connector.name } })}
    </p>
    <Button
      variant="primary-solid"
      size="sm"
      onclick={onconnect}
      disabled={loading}
      data-testid="oauth-connect-{connector.id}"
    >
      {#snippet icon()}<Plus size={12} />{/snippet}
      {$t("connections.connect_account")}
    </Button>
  </Card>
{:else}
  <Card class="max-w-2xl overflow-hidden">
    {#each accounts as account, i (account.account_id)}
      <div
        class="px-4 py-3 {i === accounts.length - 1 ? '' : 'border-b border-border/40'}"
      >
      <div class="flex items-center gap-3">
        <div class="min-w-0 flex-1">
          <div class="truncate text-body-sm font-medium text-foreground">{account.account_id}</div>
          <div class="mt-0.5 text-caption text-muted-foreground">
            {$t("connections.token_encrypted_keyring")}
          </div>
        </div>
        <Button
          variant="ghost"
          size="sm"
          onclick={() => ondisconnect(account)}
          disabled={loading}
          class="text-destructive hover:bg-destructive/10"
          data-testid="oauth-disconnect-{account.account_id}"
        >
          {$t("connections.disconnect")}
        </Button>
      </div>
      {#if showsDriveFolders}
        <AccountDriveFolder
          accountId={account.account_id}
          folder={driveFolderFor(driveFolders, account.account_id)}
          onchanged={() => void refreshDriveFolders()}
        />
      {/if}
      {#if showsOneDriveChoices}
        <AccountOneDriveChoices accountId={account.account_id} />
      {/if}
      </div>
    {/each}
  </Card>
{/if}
