<script lang="ts">
  /**
   * NativeProviderCard - the card of a provider whose account is not
   * connected yet: what it brings, and Connect. Each account connected then
   * has its own card. When no client is set up for the provider, connecting
   * cannot complete, so the card says so instead of offering it; the client
   * itself is entered in the developer mode.
   */
  import { t } from "svelte-i18n";
  import { Plus, UserRound } from "lucide-svelte";
  import { DetailHeader } from "$lib/components/operator";
  import { DeveloperOnly } from "$lib/components/shared";
  import { Banner } from "$lib/components/ui/banner";
  import { Button } from "$lib/components/ui/button";
  import { Card } from "$lib/components/ui/card";
  import type { OauthClientIdStatus } from "$lib/ipc/oauthClients";
  import type { NativeConnectorCard } from "../shared/types";
  import NativeClientSection from "./NativeClientSection.svelte";

  interface Props {
    connector: NativeConnectorCard;
    /** The provider has no usable client yet, so connecting cannot work. */
    needsSetup: boolean;
    /** The provider's client; `undefined` while it could not be read. */
    clientStatus: OauthClientIdStatus | undefined;
    /** Why the client could not be read, if it could not. */
    clientError: string | null;
    loading: boolean;
    /** Read the client again. */
    onclientretry: () => void;
    onconnect: () => void;
  }

  let { connector, needsSetup, clientStatus, clientError, loading, onclientretry, onconnect }: Props = $props();
</script>

<DetailHeader title={connector.name} titleTestid="connection-detail-title" meta={connector.description}>
  {#snippet leading()}
    <span class="inline-flex h-10 w-10 items-center justify-center rounded-lg bg-muted text-foreground">
      <UserRound size={18} aria-hidden="true" />
    </span>
  {/snippet}
</DetailHeader>

<div class="flex-1 space-y-4 overflow-auto px-8 py-5">
  {#if needsSetup}
    <Banner
      variant="warning"
      title={$t("connections.needs_client_title")}
      class="max-w-2xl"
      data-testid="native-needs-config-{connector.id}"
    >
      {$t("connections.needs_client_body", { values: { provider: connector.name } })}
    </Banner>
  {:else}
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
  {/if}

  <DeveloperOnly>
    <NativeClientSection
      provider={connector.id}
      status={clientStatus}
      loadError={clientError}
      {loading}
      onretry={onclientretry}
      onchanged={onclientretry}
    />
  </DeveloperOnly>
</div>
