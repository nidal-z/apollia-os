<script lang="ts">
  import { onMount } from "svelte";
  import { t } from "svelte-i18n";
  import { Key, Lock, Plug, ArrowRight } from "lucide-svelte";
  import { ErrorBanner } from "$lib/components/operator";
  import { humanize } from "$lib/errors/humanize";
  import SettingsSubPage from "../../components/settings/SettingsSubPage.svelte";
  import SettingSectionSkeleton from "../../components/settings/SettingSectionSkeleton.svelte";
  import OauthProviderCard from "../../components/integrations/OauthProviderCard.svelte";
  import OauthCredentialRow from "../../components/integrations/OauthCredentialRow.svelte";
  import OauthDriveFolders from "../../components/integrations/OauthDriveFolders.svelte";
  import {
    oauthListClientIds,
    oauthSetClientId,
    oauthSetClientSecret,
    oauthSetApiKey,
    type OauthClientIdStatus,
  } from "$lib/ipc/oauthClients";

  const REDIRECT_URI = "http://127.0.0.1/";

  let statuses = $state<OauthClientIdStatus[]>([]);
  let loading = $state(true);
  let loaded = $state(false);
  let rawError = $state<string | null>(null);

  const loadError = $derived(rawError ? humanize(rawError, $t) : null);

  async function refresh(): Promise<void> {
    loading = true;
    rawError = null;
    try {
      statuses = await oauthListClientIds();
      loaded = true;
    } catch (err) {
      rawError = err instanceof Error ? err.message : String(err);
    } finally {
      loading = false;
    }
  }

  function providerName(provider: string): string {
    const key = `settings.integrations.provider.${provider}`;
    const label = $t(key);
    return label === key ? provider : label;
  }

  function clientIdPlaceholder(provider: string): string {
    return $t(`settings.integrations.placeholder.client_id_${provider}`);
  }

  onMount(refresh);
</script>

<SettingsSubPage route="integrations" data-testid="settings-integrations-oauth">
  {#if loading && !loaded}
    <SettingSectionSkeleton />
  {:else if loadError}
    <ErrorBanner
      tone="danger"
      onretry={refresh}
      retryLabel={$t("common.retry")}
      {loading}
      data-testid="oauth-settings-error"
    >
      <p class="font-medium">{$t("settings.integrations.load_error_title")}</p>
      <p class="text-caption opacity-90">{$t("settings.integrations.load_error_body")}</p>
    </ErrorBanner>
  {:else if statuses.length === 0}
    <div
      class="flex flex-col items-center gap-2.5 rounded-xl border border-dashed border-border bg-surface-2/30 px-6 py-11 text-center"
      data-testid="oauth-settings-empty"
    >
      <span class="flex h-10 w-10 items-center justify-center rounded-lg bg-primary/10 text-primary">
        <Plug size={20} strokeWidth={1.75} aria-hidden="true" />
      </span>
      <p class="text-body-sm font-medium text-foreground">
        {$t("settings.integrations.empty_title")}
      </p>
      <p class="max-w-sm text-caption text-muted-foreground">
        {$t("settings.integrations.empty_body")}
      </p>
    </div>
  {:else}
    <div
      class="rounded-lg border border-border/60 bg-surface-2/40 px-4 py-3"
      data-testid="oauth-resolution-explainer"
    >
      <p class="max-w-prose text-caption text-muted-foreground">
        {$t("settings.integrations.explain_intro")}
      </p>
      <div class="mt-2.5 flex flex-wrap items-center gap-2 text-caption" aria-hidden="true">
        <span class="rounded-full leading-none border border-border bg-surface-1 px-2.5 py-1">
          <!-- i18n-ignore: environment variable name -->
          <span class="font-mono">APOLLIA_*_CLIENT_ID</span>
          <span class="text-foreground/80">{$t("settings.integrations.source.env")}</span>
        </span>
        <ArrowRight size={13} class="text-muted-foreground/60" />
        <span class="rounded-full leading-none border border-primary/40 bg-primary/8 px-2.5 py-1 text-primary">
          <span class="font-mono">oauth-clients.toml</span>
          <span>{$t("settings.integrations.source.file")}</span>
        </span>
        <ArrowRight size={13} class="text-muted-foreground/60" />
        <span class="rounded-full leading-none border border-border bg-surface-1 px-2.5 py-1">
          {$t("settings.integrations.source.builtin")}
        </span>
      </div>
    </div>

    {#each statuses as status (status.provider)}
      {@const provider = status.provider}
      {@const isGoogle = provider === "google"}
      <OauthProviderCard
        {status}
        name={providerName(provider)}
        meta={$t(`settings.integrations.provider_meta.${provider}`)}
        redirectUri={isGoogle ? REDIRECT_URI : undefined}
        footerNote={isGoogle
          ? $t("settings.integrations.stored_note")
          : $t("settings.integrations.ms_no_secret")}
        footerInfo={!isGoogle}
        onimported={() => void refresh()}
      >
        <OauthCredentialRow
          mode="public"
          label={$t("settings.integrations.field.client_id")}
          required
          source={status.source}
          configured={status.effective_client_id.length > 0}
          currentValue={status.effective_client_id}
          placeholder={clientIdPlaceholder(provider)}
          help={isGoogle ? undefined : $t("settings.integrations.help.ms_builtin")}
          onsave={async (v) => {
            await oauthSetClientId(provider, v);
            await refresh();
          }}
          inputTestid={`oauth-input-${provider}`}
          saveTestid={`oauth-save-${provider}`}
          data-testid={`oauth-field-client-id-${provider}`}
        >
          {#snippet icon()}<Key size={13} strokeWidth={1.75} aria-hidden="true" />{/snippet}
        </OauthCredentialRow>

        {#if status.requires_client_secret}
          <OauthCredentialRow
            mode="secret"
            label={$t("settings.integrations.field.client_secret")}
            required
            source={status.client_secret_source}
            configured={status.has_client_secret}
            placeholder={$t("settings.integrations.placeholder.client_secret")}
            help={$t("settings.integrations.help.client_secret")}
            onsave={async (v) => {
              await oauthSetClientSecret(provider, v);
              await refresh();
            }}
            ondelete={async () => {
              await oauthSetClientSecret(provider, "");
              await refresh();
            }}
            inputTestid={`oauth-secret-input-${provider}`}
            saveTestid={`oauth-save-secret-${provider}`}
            deleteTestid={`oauth-clear-secret-${provider}`}
            data-testid={`oauth-field-client-secret-${provider}`}
          >
            {#snippet icon()}<Lock size={13} strokeWidth={1.75} aria-hidden="true" />{/snippet}
          </OauthCredentialRow>
        {/if}

        {#if status.requires_api_key}
          <OauthCredentialRow
            mode="secret"
            label={$t("settings.integrations.field.api_key")}
            source={status.api_key_source}
            configured={status.has_api_key}
            placeholder={$t("settings.integrations.placeholder.api_key")}
            help={$t("settings.integrations.help.api_key")}
            onsave={async (v) => {
              await oauthSetApiKey(provider, v);
              await refresh();
            }}
            ondelete={async () => {
              await oauthSetApiKey(provider, "");
              await refresh();
            }}
            inputTestid={`oauth-api-key-input-${provider}`}
            saveTestid={`oauth-save-api-key-${provider}`}
            deleteTestid={`oauth-clear-api-key-${provider}`}
            data-testid={`oauth-field-api-key-${provider}`}
          >
            {#snippet icon()}<Key size={13} strokeWidth={1.75} aria-hidden="true" />{/snippet}
          </OauthCredentialRow>
        {/if}
      </OauthProviderCard>
    {/each}

    <OauthDriveFolders />
  {/if}
</SettingsSubPage>
