<script lang="ts">
  import { t } from "svelte-i18n";
  import { Sheet, SheetHeader, SheetContent, SheetFooter } from "$lib/components/ui/sheet";
  import { Button } from "$lib/components/ui/button";
  import { Checkbox } from "$lib/components/ui/checkbox";
  import { RadioGroup, RadioItem } from "$lib/components/ui/radio";
  import CredentialField from "./CredentialField.svelte";
  import { Input } from "$lib/components/ui/input";
  import {
    WEB_SEARCH_DEFAULT_CONFIG,
    WEB_READ_DEFAULT_CONFIG,
  } from "./tools/toolCatalog";
  import {
    getToolConfig,
    updateToolConfig,
    type ToolStatusDto,
  } from "$lib/stores/toolGovernance";

  interface Props {
    open: boolean;
    tool: ToolStatusDto | null;
    /** Humanized tool name for the header; falls back to the raw tool id. */
    title?: string;
    onclose: () => void;
  }

  let { open, tool, title, onclose }: Props = $props();

  const displayName = $derived(title || tool?.name || "");

  // ─── Web search form state ────────────────────────────────────────────
  type WebSearchBackend = "auto" | "duckduckgo" | "brave";

  interface WebSearchFormState {
    backend: WebSearchBackend;
    require_configured: boolean;
    brave_timeout_secs: number;
    brave_max_results: number;
    ddg_timeout_secs: number;
    ddg_max_response_kb: number;
  }

  interface WebReadFormState {
    timeout_secs: number;
    max_response_kb: number;
    ssrf_guard: boolean;
  }

  const DEFAULT_WEB_SEARCH: WebSearchFormState = {
    backend: WEB_SEARCH_DEFAULT_CONFIG.backend,
    require_configured: WEB_SEARCH_DEFAULT_CONFIG.require_configured,
    brave_timeout_secs: WEB_SEARCH_DEFAULT_CONFIG.brave.timeout_secs,
    brave_max_results: WEB_SEARCH_DEFAULT_CONFIG.brave.max_results,
    ddg_timeout_secs: WEB_SEARCH_DEFAULT_CONFIG.duckduckgo.timeout_secs,
    ddg_max_response_kb: WEB_SEARCH_DEFAULT_CONFIG.duckduckgo.max_response_kb,
  };

  const DEFAULT_WEB_READ: WebReadFormState = {
    timeout_secs: WEB_READ_DEFAULT_CONFIG.timeout_secs,
    max_response_kb: WEB_READ_DEFAULT_CONFIG.max_response_kb,
    ssrf_guard: WEB_READ_DEFAULT_CONFIG.ssrf_guard,
  };

  let webSearch = $state<WebSearchFormState>({ ...DEFAULT_WEB_SEARCH });
  let webRead = $state<WebReadFormState>({ ...DEFAULT_WEB_READ });
  let loadedFor = $state<string | null>(null);
  let loading = $state(false);
  let saving = $state(false);
  let error = $state<string | null>(null);

  function clamp(value: number, min: number, max: number): number {
    if (Number.isNaN(value)) return min;
    return Math.max(min, Math.min(max, Math.trunc(value)));
  }

  function readNumber(
    obj: Record<string, unknown> | null | undefined,
    key: string,
    fallback: number,
  ): number {
    if (!obj) return fallback;
    const v = obj[key];
    return typeof v === "number" && Number.isFinite(v) ? v : fallback;
  }

  function readBool(
    obj: Record<string, unknown> | null | undefined,
    key: string,
    fallback: boolean,
  ): boolean {
    if (!obj) return fallback;
    const v = obj[key];
    return typeof v === "boolean" ? v : fallback;
  }

  function asRecord(v: unknown): Record<string, unknown> | null {
    return v && typeof v === "object" && !Array.isArray(v)
      ? (v as Record<string, unknown>)
      : null;
  }

  function applyWebSearchConfig(cfg: Record<string, unknown> | null): void {
    const backendRaw = cfg?.backend;
    const backend: WebSearchBackend =
      backendRaw === "duckduckgo" || backendRaw === "brave"
        ? backendRaw
        : "auto";
    const brave = asRecord(cfg?.brave);
    const ddg = asRecord(cfg?.duckduckgo);
    webSearch = {
      backend,
      require_configured: readBool(cfg, "require_configured", DEFAULT_WEB_SEARCH.require_configured),
      brave_timeout_secs: readNumber(brave, "timeout_secs", DEFAULT_WEB_SEARCH.brave_timeout_secs),
      brave_max_results: readNumber(brave, "max_results", DEFAULT_WEB_SEARCH.brave_max_results),
      ddg_timeout_secs: readNumber(ddg, "timeout_secs", DEFAULT_WEB_SEARCH.ddg_timeout_secs),
      ddg_max_response_kb: readNumber(ddg, "max_response_kb", DEFAULT_WEB_SEARCH.ddg_max_response_kb),
    };
  }

  function applyWebReadConfig(cfg: Record<string, unknown> | null): void {
    webRead = {
      timeout_secs: readNumber(cfg, "timeout_secs", DEFAULT_WEB_READ.timeout_secs),
      max_response_kb: readNumber(cfg, "max_response_kb", DEFAULT_WEB_READ.max_response_kb),
      ssrf_guard: readBool(cfg, "ssrf_guard", DEFAULT_WEB_READ.ssrf_guard),
    };
  }

  $effect(() => {
    if (!open || !tool) return;
    if (loadedFor === tool.name) return;
    loadedFor = tool.name;
    loading = true;
    error = null;
    void getToolConfig(tool.name)
      .then((cfg) => {
        if (tool?.name === "web_search") applyWebSearchConfig(cfg);
        else if (tool?.name === "web_read") applyWebReadConfig(cfg);
      })
      .catch((err: unknown) => {
        error = err instanceof Error ? err.message : String(err);
      })
      .finally(() => {
        loading = false;
      });
  });

  $effect(() => {
    if (!open) {
      loadedFor = null;
      error = null;
    }
  });

  function buildWebSearchPayload(): Record<string, unknown> {
    return {
      backend: webSearch.backend,
      require_configured: webSearch.require_configured,
      brave: {
        timeout_secs: clamp(webSearch.brave_timeout_secs, 1, 120),
        max_results: clamp(webSearch.brave_max_results, 1, 20),
      },
      duckduckgo: {
        timeout_secs: clamp(webSearch.ddg_timeout_secs, 1, 120),
        max_response_kb: clamp(webSearch.ddg_max_response_kb, 16, 16_384),
      },
    };
  }

  function buildWebReadPayload(): Record<string, unknown> {
    return {
      timeout_secs: clamp(webRead.timeout_secs, 1, 120),
      max_response_kb: clamp(webRead.max_response_kb, 64, 32_768),
      ssrf_guard: webRead.ssrf_guard,
    };
  }

  async function save(): Promise<void> {
    if (!tool) return;
    saving = true;
    error = null;
    try {
      let payload: Record<string, unknown>;
      if (tool.name === "web_search") payload = buildWebSearchPayload();
      else if (tool.name === "web_read") payload = buildWebReadPayload();
      else {
        saving = false;
        return;
      }
      await updateToolConfig(tool.name, payload);
      onclose();
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      saving = false;
    }
  }

  const hasConfigForm = $derived(
    tool?.name === "web_search" || tool?.name === "web_read",
  );

  const braveConfigured = $derived(
    tool?.credential_keys.includes("brave.api_key") ?? false,
  );
</script>

<Sheet {open} {onclose} width="lg">
  <SheetHeader
    title={$t("settings.tool_config.title", { values: { name: displayName } })}
    {onclose}
    closeLabel={$t("common.close")}
    class="px-5 py-4 items-center"
  >
    {#snippet titleSlot()}
      <div class="space-y-0.5">
        <h2 class="text-lg font-semibold">
          {$t("settings.tool_config.title", { values: { name: displayName } })}
        </h2>
        {#if tool?.active_backend}
          <p class="text-xs text-muted-foreground">
            {$t("settings.tool_config.active_backend")} <code class="rounded bg-muted/40 px-1">{tool.active_backend}</code>
          </p>
        {/if}
      </div>
    {/snippet}
  </SheetHeader>

  <SheetContent padding="flush" class="px-5 py-5 space-y-6" data-testid="tool-config-drawer-body">
      {#if loading}
        <p class="text-sm text-muted-foreground">{$t("common.loading")}</p>
      {:else if !tool}
        <p class="text-sm text-muted-foreground">{$t("settings.tool_config.no_tool_selected")}</p>
      {:else if tool.name === "web_search"}
        <section class="space-y-3">
          <h3 class="text-sm font-medium">{$t("settings.tool_config.web_search.preferred_backend")}</h3>
          <RadioGroup
            value={webSearch.backend}
            onchange={(v) => (webSearch.backend = v as WebSearchBackend)}
            data-testid="web-search-backend-group"
          >
            <RadioItem
              value="auto"
              checked={webSearch.backend === "auto"}
              onchange={(v) => (webSearch.backend = v as WebSearchBackend)}
            >
              {$t("settings.tool_config.web_search.backend_auto")}
            </RadioItem>
            <RadioItem
              value="duckduckgo"
              checked={webSearch.backend === "duckduckgo"}
              onchange={(v) => (webSearch.backend = v as WebSearchBackend)}
            >
              {$t("settings.tool_config.web_search.backend_ddg_only")}
            </RadioItem>
            <RadioItem
              value="brave"
              checked={webSearch.backend === "brave"}
              onchange={(v) => (webSearch.backend = v as WebSearchBackend)}
            >
              {$t("settings.tool_config.web_search.backend_brave_only")}
            </RadioItem>
          </RadioGroup>
        </section>

        <section class="space-y-3">
          <h3 class="text-sm font-medium">Brave Search</h3>
          <CredentialField
            toolName="web_search"
            keyName="brave.api_key"
            label={$t("settings.tool_config.web_search.brave_api_key")}
            configured={braveConfigured}
            canTest
            data-testid="brave-api-key"
          />
          <div class="grid grid-cols-2 gap-3">
            <label class="space-y-1 text-sm">
              <span class="text-muted-foreground">{$t("settings.tool_config.field.timeout_secs")}</span>
              <Input
                type="number"
                min="1"
                max="120"
                bind:value={webSearch.brave_timeout_secs}
                class="flex h-10 w-full rounded-md border border-border bg-background px-3 text-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                data-testid="brave-timeout"
               />
            </label>
            <label class="space-y-1 text-sm">
              <span class="text-muted-foreground">{$t("settings.tool_config.field.max_results")}</span>
              <Input
                type="number"
                min="1"
                max="20"
                bind:value={webSearch.brave_max_results}
                class="flex h-10 w-full rounded-md border border-border bg-background px-3 text-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                data-testid="brave-max-results"
               />
            </label>
          </div>
        </section>

        <section class="space-y-3">
          <h3 class="text-sm font-medium">DuckDuckGo</h3>
          <div class="grid grid-cols-2 gap-3">
            <label class="space-y-1 text-sm">
              <span class="text-muted-foreground">{$t("settings.tool_config.field.timeout_secs")}</span>
              <Input
                type="number"
                min="1"
                max="120"
                bind:value={webSearch.ddg_timeout_secs}
                class="flex h-10 w-full rounded-md border border-border bg-background px-3 text-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                data-testid="ddg-timeout"
               />
            </label>
            <label class="space-y-1 text-sm">
              <span class="text-muted-foreground">{$t("settings.tool_config.field.max_response_kb")}</span>
              <Input
                type="number"
                min="16"
                max="16384"
                bind:value={webSearch.ddg_max_response_kb}
                class="flex h-10 w-full rounded-md border border-border bg-background px-3 text-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                data-testid="ddg-max-response-kb"
               />
            </label>
          </div>
        </section>

        <section class="space-y-2">
          <label class="flex items-center gap-2 text-sm">
            <Checkbox
              checked={webSearch.require_configured}
              onchange={(c) => (webSearch.require_configured = c)}
              data-testid="require-configured"
            />
            <span>
              {$t("settings.tool_config.web_search.require_configured")}
              <span class="block text-xs text-muted-foreground">
                {$t("settings.tool_config.web_search.require_configured_desc")}
              </span>
            </span>
          </label>
        </section>
      {:else if tool.name === "web_read"}
        <section class="space-y-3">
          <div class="grid grid-cols-2 gap-3">
            <label class="space-y-1 text-sm">
              <span class="text-muted-foreground">{$t("settings.tool_config.field.timeout_secs")}</span>
              <Input
                type="number"
                min="1"
                max="120"
                bind:value={webRead.timeout_secs}
                class="flex h-10 w-full rounded-md border border-border bg-background px-3 text-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                data-testid="web-read-timeout"
               />
            </label>
            <label class="space-y-1 text-sm">
              <span class="text-muted-foreground">{$t("settings.tool_config.field.max_response_kb")}</span>
              <Input
                type="number"
                min="64"
                max="32768"
                bind:value={webRead.max_response_kb}
                class="flex h-10 w-full rounded-md border border-border bg-background px-3 text-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                data-testid="web-read-max-response-kb"
               />
            </label>
          </div>
          <label class="flex items-center gap-2 text-sm">
            <Checkbox
              checked={webRead.ssrf_guard}
              onchange={(c) => (webRead.ssrf_guard = c)}
              data-testid="web-read-ssrf-guard"
            />
            <span>
              {$t("settings.tool_config.web_read.ssrf_guard")}
            </span>
          </label>
        </section>
      {:else}
        <p class="text-sm text-muted-foreground">
          {$t("settings.tool_config.no_config")}
        </p>
      {/if}

      {#if error}
        <p class="text-sm text-destructive" data-testid="tool-config-error">{error}</p>
      {/if}
  </SheetContent>

  <SheetFooter class="px-5 py-3">
    <Button variant="ghost" size="sm" onclick={onclose}>{$t("common.cancel")}</Button>
    {#if hasConfigForm}
      <Button onclick={save} loading={saving} disabled={saving} data-testid="tool-config-save">
        {$t("common.save")}
      </Button>
    {/if}
  </SheetFooter>
</Sheet>
