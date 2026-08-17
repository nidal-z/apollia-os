<script lang="ts">
  /**
   * CustomMcpForm - the "custom MCP server" builder (transport, env, approval).
   *
   * Self-contained: owns its form state and the test / install IPC calls, and
   * emits `oninstalled` once a server is added so the parent can refresh + close.
   */
  import { t } from "svelte-i18n";
  import { ErrorBanner } from "$lib/components/operator";
  import { Banner } from "$lib/components/ui/banner";
  import { Input } from "$lib/components/ui/input";
  import { Select } from "$lib/components/ui/select";
  import { Textarea } from "$lib/components/ui/textarea";
  import { Checkbox } from "$lib/components/ui/checkbox";
  import { Button } from "$lib/components/ui/button";
  import { addMcpServer, testMcpConnection } from "$lib/ipc/connections";
  import { formatTauriError } from "$lib/connections/errors";
  import type { CustomTransport } from "../shared/types";

  interface Props {
    oninstalled: () => void;
  }

  let { oninstalled }: Props = $props();

  let form = $state({
    name: "",
    transport: "stdio" as CustomTransport,
    command: "",
    args: "",
    url: "",
    headers: "",
    requires_approval: true,
  });
  let busy = $state(false);
  let error = $state<string | null>(null);
  let testResult = $state<string | null>(null);

  function sanitizeServerName(raw: string): string {
    const cleaned = raw
      .toLowerCase()
      .replace(/[^a-z0-9_-]+/g, "-")
      .replace(/-+/g, "-")
      .replace(/^-|-$/g, "");
    return cleaned.length > 0 ? cleaned : "custom-mcp";
  }

  function buildConfig(): Record<string, unknown> | null {
    const env: Record<string, string> = {};
    for (const line of form.headers.split(/\r?\n/)) {
      const trimmed = line.trim();
      if (!trimmed || trimmed.startsWith("#")) continue;
      const eq = trimmed.indexOf("=");
      if (eq <= 0) continue;
      env[trimmed.slice(0, eq).trim()] = trimmed.slice(eq + 1).trim();
    }
    const base: Record<string, unknown> = {
      name: sanitizeServerName(form.name || "custom-mcp"),
      env,
      transport: form.transport,
      requires_approval: form.requires_approval,
      tags: [],
    };
    if (form.transport === "stdio") {
      if (!form.command.trim()) {
        error = $t("connections.custom_error_command_required");
        return null;
      }
      base.command = form.command.trim();
      base.args = form.args
        .split(/\s+/)
        .map((a) => a.trim())
        .filter((a) => a.length > 0);
    } else {
      if (!form.url.trim()) {
        error = $t("connections.custom_error_url_required");
        return null;
      }
      base.url = form.url.trim();
    }
    return base;
  }

  async function handleTest(): Promise<void> {
    error = null;
    testResult = null;
    const config = buildConfig();
    if (!config) return;
    busy = true;
    try {
      const response = await testMcpConnection(config);
      if (response.kind === "success") {
        testResult = $t("connections.custom_test_tools_detected", {
          values: { count: response.tools.length },
        });
      } else {
        error = $t("connections.custom_error_oauth_required");
      }
    } catch (e) {
      error = formatTauriError(e, $t);
    } finally {
      busy = false;
    }
  }

  async function handleInstall(): Promise<void> {
    error = null;
    const config = buildConfig();
    if (!config) return;
    busy = true;
    try {
      await addMcpServer(config);
      oninstalled();
    } catch (e) {
      error = formatTauriError(e, $t);
    } finally {
      busy = false;
    }
  }
</script>

<div class="max-w-2xl flex-1 overflow-auto px-6 pb-6 pt-4">
  <p class="mb-4 text-body-xs text-muted-foreground">{$t("connections.custom_intro")}</p>

  <div class="space-y-3">
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

    <label class="block text-label-sm font-medium text-foreground">
      {$t("connections.custom_transport_label")}
      <Select class="mt-1" bind:value={form.transport} disabled={busy} data-testid="custom-mcp-transport-select">
        <option value="stdio">{$t("connections.custom_transport_stdio")}</option>
        <option value="streamable-http">{$t("connections.custom_transport_http")}</option>
        <option value="sse">{$t("connections.custom_transport_sse")}</option>
      </Select>
    </label>

    {#if form.transport === "stdio"}
      <label class="block text-label-sm font-medium text-foreground">
        {$t("connections.custom_command_label")}
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
        {$t("connections.custom_args_label")}
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
          placeholder="https://mcp.example.com/v1"
          bind:value={form.url}
          disabled={busy}
          data-testid="custom-mcp-url-input"
        />
      </label>
    {/if}

    <label class="block text-label-sm font-medium text-foreground">
      {form.transport === "stdio"
        ? $t("connections.custom_env_label")
        : $t("connections.custom_headers_label")}
      <span class="font-normal text-muted-foreground">{@html $t("connections.custom_env_hint_html")}</span>
      <Textarea
        class="mt-1 font-mono"
        rows={3}
        placeholder={form.transport === "stdio"
          ? "NOTION_TOKEN=ntn_xxxxx\nDEBUG=1"
          : "Authorization=Bearer xxx\nX-Custom-Header=value"}
        bind:value={form.headers}
        disabled={busy}
        data-testid="custom-mcp-env-input"
      />
    </label>

    <label class="flex items-center gap-2 pt-1 text-label-sm font-medium text-foreground">
      <Checkbox bind:checked={form.requires_approval} disabled={busy} data-testid="custom-mcp-approval-toggle" />
      {$t("connections.custom_require_approval")}
    </label>
  </div>

  {#if testResult}
    <Banner variant="success" class="mt-4" data-testid="custom-mcp-test-ok">
      {$t("connections.custom_test_ok", { values: { result: testResult } })}
    </Banner>
  {/if}
  {#if error}
    <ErrorBanner message={error} class="mt-4" />
  {/if}

  <div class="mt-5 flex justify-end gap-2">
    <Button variant="outline" size="sm" onclick={handleTest} disabled={busy} data-testid="custom-mcp-test-btn">
      {busy ? $t("connections.testing") : $t("connections.test")}
    </Button>
    <Button variant="primary-solid" size="sm" onclick={handleInstall} disabled={busy} data-testid="custom-mcp-install-btn">
      {busy ? $t("connections.installing") : $t("connections.install")}
    </Button>
  </div>
</div>
