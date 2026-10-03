<script lang="ts">
  /**
   * NativeClientSection - the OAuth client of a native connection, on its
   * page: its state and "Retry", where the client id comes from, the client id
   * and secret, "Import the client JSON", "Test the client", "Copy the
   * redirect address", and for Google the API key. The gestures live in
   * `$lib/connections/oauthClient`; the secret and the key go to the secret
   * store, never to a file.
   */
  import { t } from "svelte-i18n";
  import { Key, Lock, Upload, Check, Link2 } from "lucide-svelte";
  import { open as openFilePicker } from "@tauri-apps/plugin-dialog";
  import { ErrorBanner } from "$lib/components/operator";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import { Card } from "$lib/components/ui/card";
  import { reportError } from "$lib/errors/reportError";
  import { oauthReadiness } from "$lib/connections/status";
  import {
    GOOGLE_REDIRECT_URI,
    clearApiKey,
    clearClientSecret,
    clientIdOriginKey,
    copyRedirect,
    importClientJson,
    oauthClientFields,
    saveApiKey,
    saveClientId,
    saveClientSecret,
    testClient,
  } from "$lib/connections/oauthClient";
  import type { ProviderId } from "$lib/connections/capabilities";
  import type { OauthClientIdStatus } from "$lib/ipc/oauthClients";
  import OauthCredentialRow from "../../integrations/OauthCredentialRow.svelte";

  interface Props {
    provider: ProviderId;
    /** The client's status; `undefined` while it could not be read. */
    status: OauthClientIdStatus | undefined;
    /** Why the status could not be read, if it could not. */
    loadError: string | null;
    loading: boolean;
    /** Read the status again. */
    onretry: () => void;
    /** A gesture changed the client: the parent reads it again. */
    onchanged: () => void;
  }

  let { provider, status, loadError, loading, onretry, onchanged }: Props = $props();

  const fields = $derived(status ? oauthClientFields(status) : null);
  const readiness = $derived(status ? oauthReadiness(status) : "none");

  type TestOutcome = "idle" | "testing" | "valid" | "warning" | "error";
  let testOutcome = $state<TestOutcome>("idle");
  let testMessage = $state("");
  let importing = $state(false);
  let importError = $state("");
  let copied = $state(false);

  async function changed<T>(work: Promise<T>): Promise<T> {
    const result = await work;
    onchanged();
    return result;
  }

  async function runTest(): Promise<void> {
    testOutcome = "testing";
    testMessage = "";
    try {
      const result = await testClient(provider);
      testOutcome = result.ok ? "valid" : "warning";
      testMessage = result.detail;
    } catch (err) {
      testOutcome = "error";
      testMessage = reportError(err, { surface: "inline" }).friendly_message;
    }
  }

  async function runImport(): Promise<void> {
    const picked = await openFilePicker({
      multiple: false,
      directory: false,
      filters: [{ name: "JSON", extensions: ["json"] }],
    });
    importing = true;
    importError = "";
    try {
      const imported = await importClientJson(provider, typeof picked === "string" ? picked : null);
      if (imported) onchanged();
    } catch (err) {
      importError = reportError(err, { surface: "inline" }).friendly_message;
    } finally {
      importing = false;
    }
  }

  async function runCopy(): Promise<void> {
    copied = await copyRedirect(navigator.clipboard);
    if (copied) setTimeout(() => (copied = false), 1500);
  }
</script>

<Card class="max-w-2xl px-4 py-3.5" id="oauth-client-{provider}" data-testid="native-client-{provider}">
  <div class="mb-1 text-label-md font-semibold text-foreground">
    {$t(`connections.oauth_client.title.${provider}`)}
  </div>
  <p class="mb-3 text-caption text-muted-foreground">
    {$t(`connections.oauth_client.intro.${provider}`)}
  </p>

  {#if !status && loadError}
    <ErrorBanner
      tone="danger"
      onretry={onretry}
      retryLabel={$t("common.retry")}
      {loading}
      data-testid="native-client-error-{provider}"
    >
      <p class="font-medium">{$t("connections.oauth_client.load_error")}</p>
      <p class="text-caption opacity-90">{loadError}</p>
    </ErrorBanner>
  {:else if status && fields}
    <div class="mb-3 flex flex-wrap items-center gap-2" data-testid="native-client-state-{provider}">
      <Badge
        variant={readiness === "ready" ? "success" : readiness === "partial" ? "warning" : "danger"}
        size="sm"
      >
        {$t(`settings.integrations.status.${readiness}`)}
      </Badge>
      <Button
        variant="ghost"
        size="sm"
        onclick={onretry}
        disabled={loading}
        data-testid="native-client-retry-{provider}"
      >
        {$t("common.retry")}
      </Button>
    </div>

    <p class="mb-1 text-caption text-muted-foreground" data-testid="native-client-origin-{provider}">
      {$t(clientIdOriginKey(status.source))}
    </p>

    <OauthCredentialRow
      mode="public"
      label={$t("settings.integrations.field.client_id")}
      required={provider === "google"}
      source={status.source}
      configured={status.effective_client_id.length > 0}
      currentValue={status.effective_client_id}
      placeholder={$t(`settings.integrations.placeholder.client_id_${provider}`)}
      onsave={(v) => changed(saveClientId(provider, v))}
      inputTestid="native-client-id-input-{provider}"
      saveTestid="native-client-id-save-{provider}"
      data-testid="native-client-id-{provider}"
    >
      {#snippet icon()}<Key size={13} strokeWidth={1.75} aria-hidden="true" />{/snippet}
    </OauthCredentialRow>

    {#if fields.secret}
      <OauthCredentialRow
        mode="secret"
        label={$t("settings.integrations.field.client_secret")}
        required={fields.secretRequired}
        source={status.client_secret_source}
        configured={status.has_client_secret}
        placeholder={$t("settings.integrations.placeholder.client_secret")}
        help={$t("connections.oauth_client.secret_kept")}
        onsave={(v) => changed(saveClientSecret(provider, v))}
        ondelete={() => changed(clearClientSecret(provider))}
        inputTestid="native-client-secret-input-{provider}"
        saveTestid="native-client-secret-save-{provider}"
        deleteTestid="native-client-secret-clear-{provider}"
        data-testid="native-client-secret-{provider}"
      >
        {#snippet icon()}<Lock size={13} strokeWidth={1.75} aria-hidden="true" />{/snippet}
      </OauthCredentialRow>
    {/if}

    <div class="mt-3 flex flex-wrap items-center gap-2">
      {#if fields.importJson}
        <Button
          variant="outline"
          size="sm"
          onclick={runImport}
          loading={importing}
          disabled={importing}
          data-testid="native-client-import-{provider}"
        >
          {#snippet icon()}<Upload size={12} />{/snippet}
          {$t("connections.oauth_client.import_json")}
        </Button>
      {/if}
      <Button
        variant="outline"
        size="sm"
        onclick={runTest}
        loading={testOutcome === "testing"}
        disabled={status.effective_client_id.length === 0 || testOutcome === "testing"}
        data-testid="native-client-test-{provider}"
      >
        {$t("connections.oauth_client.test")}
      </Button>
      {#if fields.redirect}
        <Button
          variant="ghost"
          size="sm"
          onclick={runCopy}
          data-testid="native-client-copy-redirect-{provider}"
        >
          {#if copied}
            <Check size={12} aria-hidden="true" />
            {$t("settings.integrations.copied")}
          {:else}
            {$t("connections.oauth_client.copy_redirect")}
          {/if}
        </Button>
      {/if}
    </div>

    {#if importError}
      <p class="mt-2 text-caption text-danger-a11y" role="status" data-testid="native-client-import-error-{provider}">
        {importError}
      </p>
    {/if}
    {#if testOutcome !== "idle" && testOutcome !== "testing"}
      <p
        class="mt-2 text-caption {testOutcome === 'valid' ? 'text-success-a11y' : 'text-warning-a11y'}"
        role="status"
        aria-live="polite"
        data-testid="native-client-test-result-{provider}"
      >
        <span class="font-semibold">
          {testOutcome === "valid"
            ? $t("settings.integrations.test_valid")
            : $t("settings.integrations.test_invalid")}
        </span>
        {testMessage}
      </p>
    {/if}
    {#if fields.redirect}
      <p class="mt-2 flex items-center gap-1.5 text-caption text-muted-foreground">
        <Link2 size={12} aria-hidden="true" />
        {$t("connections.oauth_client.redirect_note", { values: { uri: GOOGLE_REDIRECT_URI } })}
      </p>
    {/if}
  {/if}
</Card>

{#if status && fields?.apiKey}
  <Card class="mt-4 max-w-2xl px-4 py-3.5" data-testid="native-api-key-{provider}">
    <div class="mb-1 text-label-md font-semibold text-foreground">
      {$t("connections.oauth_client.api_key_title")}
    </div>
    <p class="mb-1 text-caption text-muted-foreground">
      {$t("connections.oauth_client.api_key_intro")}
    </p>
    <OauthCredentialRow
      mode="secret"
      label={$t("settings.integrations.field.api_key")}
      source={status.api_key_source}
      configured={status.has_api_key}
      placeholder={$t("settings.integrations.placeholder.api_key")}
      help={$t("connections.oauth_client.secret_kept")}
      onsave={(v) => changed(saveApiKey(provider, v))}
      ondelete={() => changed(clearApiKey(provider))}
      inputTestid="native-api-key-input-{provider}"
      saveTestid="native-api-key-save-{provider}"
      deleteTestid="native-api-key-clear-{provider}"
      data-testid="native-api-key-row-{provider}"
    >
      {#snippet icon()}<Key size={13} strokeWidth={1.75} aria-hidden="true" />{/snippet}
    </OauthCredentialRow>
  </Card>
{/if}
