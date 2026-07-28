<script lang="ts">
  /**
   * OauthAuthStep - the awaiting-callback body of the native OAuth dialog.
   *
   * Consent hint, a live "captured automatically" banner, a manual-open link,
   * and a paste-code Disclosure fallback (canon Disclosure, not a raw details).
   */
  import { t } from "svelte-i18n";
  import { Disclosure } from "$lib/components/ui/disclosure";
  import { Banner } from "$lib/components/ui/banner";
  import { Input } from "$lib/components/ui/input";
  import { Spinner } from "$lib/components/ui/progress";

  interface Props {
    providerShort: string;
    authUrl: string;
    awaitingCallback: boolean;
    busy: boolean;
    pastedCode: string;
    error: string | null;
  }

  let {
    providerShort,
    authUrl,
    awaitingCallback,
    busy,
    pastedCode = $bindable(),
    error,
  }: Props = $props();
</script>

<div class="space-y-3">
  <p class="text-body-sm text-muted-foreground">
    {$t("connections.oauth_consent_opened", { values: { provider: providerShort } })}
  </p>

  {#if awaitingCallback}
    <Banner variant="info">
      <span class="inline-flex items-center gap-2">
        <Spinner size={14} />
        {$t("connections.oauth_awaiting_callback", { values: { provider: providerShort } })}
      </span>
    </Banner>
  {/if}

  <p class="text-caption text-muted-foreground">
    {$t("connections.oauth_window_not_opened")}
    <a class="text-primary underline" href={authUrl} target="_blank" rel="noopener noreferrer" data-testid="oauth-open-url-link">
      {$t("connections.oauth_open_url_manually")}</a
    >.
  </p>

  <Disclosure testid="oauth-paste-code">
    {#snippet summary()}
      <span class="text-caption text-muted-foreground">{$t("connections.oauth_paste_code_summary")}</span>
    {/snippet}
    {#snippet children()}
      <div class="space-y-2">
        <p class="text-caption text-muted-foreground">{@html $t("connections.oauth_paste_code_hint_html")}</p>
        <Input
          type="text"
          class="font-mono"
          placeholder={$t("connections.custom_mcp.code_placeholder")}
          bind:value={pastedCode}
          disabled={busy}
          data-testid="oauth-paste-code-input"
        />
      </div>
    {/snippet}
  </Disclosure>
  {#if error}
    <p class="text-caption text-destructive">{error}</p>
  {/if}
</div>
