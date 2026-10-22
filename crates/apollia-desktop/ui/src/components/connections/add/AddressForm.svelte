<script lang="ts">
  /**
   * AddressForm - the connect step of a service reached at its address: a
   * name, the address, and a key when the service asks one. The key travels as
   * the authorization of the service and the runtime files it in the secret
   * store, never in the configuration. The transport, a local command, its
   * headers or variables are the developer mode's.
   */
  import { t } from "svelte-i18n";
  import { KeyRound } from "lucide-svelte";
  import { developerMode } from "$lib/stores/developerMode";
  import { ErrorBanner } from "$lib/components/operator";
  import { Banner } from "$lib/components/ui/banner";
  import { Input } from "$lib/components/ui/input";
  import { Select } from "$lib/components/ui/select";
  import { Textarea } from "$lib/components/ui/textarea";
  import { Button } from "$lib/components/ui/button";
  import { testMcpConnection } from "$lib/ipc/connections";
  import { testErrorKey } from "$lib/integrations/addConnection";
  import {
    EMPTY_ADDRESS_FORM,
    addressConfig,
    asksForKey,
    type AddressForm,
  } from "$lib/connections/addFlow";
  import type { McpServerConfigInput } from "$lib/types";

  interface Props {
    oncancel: () => void;
    /** Add the service whose configuration the form built. */
    onsubmit: (config: McpServerConfigInput) => void;
  }

  let { oncancel, onsubmit }: Props = $props();

  let form = $state<AddressForm>({ ...EMPTY_ADDRESS_FORM });
  /** The key field shows once the service asked one, or the person says it does. */
  let keyShown = $state(false);
  let keyAsked = $state(false);
  let busy = $state(false);
  let error = $state<string | null>(null);
  let testResult = $state<number | null>(null);

  const stdio = $derived($developerMode && form.transport === "stdio");

  function build(): McpServerConfigInput | null {
    error = null;
    const built = addressConfig(form, $developerMode);
    if (built.kind === "missing") {
      error =
        built.field === "command"
          ? $t("connections.custom_error_command_required")
          : $t("connections.custom_error_url_required");
      return null;
    }
    return built.config;
  }

  async function test(): Promise<void> {
    testResult = null;
    const config = build();
    if (!config) return;
    busy = true;
    try {
      const response = await testMcpConnection(config);
      if (response.kind === "success") testResult = response.tools.length;
      else error = $t("connections.custom_error_oauth_required");
    } catch (e: unknown) {
      const key = testErrorKey(e instanceof Error ? e.message : String(e));
      if (asksForKey(key) && !$developerMode) {
        keyShown = true;
        keyAsked = true;
      } else {
        error = $t(key);
      }
    } finally {
      busy = false;
    }
  }

  function submit(): void {
    const config = build();
    if (config) onsubmit(config);
  }
</script>

<div class="space-y-3" data-testid="add-flow-address">
  <p class="text-body-xs text-muted-foreground">
    {$developerMode ? $t("dev.connections.custom_intro") : $t("connections.custom_intro")}
  </p>

  <label class="block text-label-sm font-medium text-foreground">
    {$t("connections.custom_name_label")}
    <Input
      type="text"
      class="mt-1"
      placeholder={$t("connections.custom_name_placeholder")}
      bind:value={form.name}
      disabled={busy}
      data-testid="custom-mcp-name-input"
    />
  </label>

  {#if $developerMode}
    <label class="block text-label-sm font-medium text-foreground">
      {$t("dev.connections.custom_transport_label")}
      <Select class="mt-1" bind:value={form.transport} disabled={busy} data-testid="custom-mcp-transport-select">
        <option value="stdio">{$t("dev.connections.custom_transport_stdio")}</option>
        <option value="streamable-http">{$t("dev.connections.custom_transport_http")}</option>
        <option value="sse">{$t("dev.connections.custom_transport_sse")}</option>
      </Select>
    </label>
  {/if}

  {#if stdio}
    <label class="block text-label-sm font-medium text-foreground">
      {$t("dev.connections.custom_command_label")}
      <!-- i18n-ignore: executable names and a filesystem path -->
      <Input
        type="text"
        class="mt-1 font-mono"
        placeholder="npx, uvx, /path/to/binary"
        bind:value={form.command}
        disabled={busy}
        data-testid="custom-mcp-command-input"
      />
    </label>
    <label class="block text-label-sm font-medium text-foreground">
      {$t("dev.connections.custom_args_label")}
      <Input
        type="text"
        class="mt-1 font-mono"
        placeholder="@modelcontextprotocol/server-filesystem /tmp"
        bind:value={form.args}
        disabled={busy}
        data-testid="custom-mcp-args-input"
      />
    </label>
  {:else}
    <label class="block text-label-sm font-medium text-foreground">
      {$t("connections.custom_url_label")}
      <!-- i18n-ignore: example endpoint URL -->
      <Input
        type="url"
        class="mt-1 font-mono"
        placeholder="https://example.com/connection"
        bind:value={form.url}
        disabled={busy}
        data-testid="custom-mcp-url-input"
      />
    </label>
  {/if}

  {#if $developerMode}
    <label class="block text-label-sm font-medium text-foreground">
      {stdio ? $t("dev.connections.custom_env_label") : $t("dev.connections.custom_headers_label")}
      <span class="font-normal text-muted-foreground">{@html $t("dev.connections.custom_env_hint_html")}</span>
      <Textarea
        class="mt-1 font-mono"
        rows={3}
        placeholder={stdio ? "NOTION_TOKEN=ntn_xxxxx\nDEBUG=1" : "Authorization=Bearer xxx\nX-Custom-Header=value"}
        bind:value={form.headers}
        disabled={busy}
        data-testid="custom-mcp-env-input"
      />
    </label>
  {:else if keyShown}
    <label class="block text-label-sm font-medium text-foreground">
      {$t("connections.add_flow.key_label")}
      {#if keyAsked}
        <span class="block font-normal text-caption text-muted-foreground">{$t("connections.add_flow.key_asked")}</span>
      {/if}
      <Input
        type="password"
        class="mt-1"
        autocomplete="off"
        bind:value={form.key}
        disabled={busy}
        data-testid="add-flow-key-input"
      />
      <span class="mt-1 block font-normal text-caption text-muted-foreground">{$t("connections.add_flow.key_vault")}</span>
    </label>
  {:else}
    <button
      type="button"
      class="flex items-center gap-1.5 text-caption text-primary underline-offset-4 hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
      onclick={() => (keyShown = true)}
      data-testid="add-flow-key-toggle"
    >
      <KeyRound size={12} aria-hidden="true" />
      {$t("connections.add_flow.key_toggle")}
    </button>
  {/if}

  {#if testResult !== null}
    <Banner variant="success" data-testid="custom-mcp-test-ok">
      {$t("connections.custom_test_ok", {
        values: { result: $t("connections.custom_test_tools_detected", { values: { count: testResult } }) },
      })}
    </Banner>
  {/if}
  {#if error}
    <ErrorBanner message={error} />
  {/if}

  <div class="flex justify-end gap-2 border-t border-border pt-4">
    <Button variant="ghost" size="sm" onclick={oncancel} disabled={busy} data-testid="wizard-cancel-btn">
      {$t("common.cancel")}
    </Button>
    <Button variant="outline" size="sm" onclick={() => void test()} disabled={busy} data-testid="custom-mcp-test-btn">
      {busy ? $t("connections.testing") : $t("connections.test")}
    </Button>
    <Button variant="primary-solid" size="sm" onclick={submit} disabled={busy} data-testid="custom-mcp-install-btn">
      {$t("connections.install")}
    </Button>
  </div>
</div>
