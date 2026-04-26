<script lang="ts">
  import { X } from "lucide-svelte";
  import { Sheet } from "$lib/components/ui/sheet";
  import { Button } from "$lib/components/ui/button";
  import { Checkbox } from "$lib/components/ui/checkbox";
  import { RadioGroup, RadioItem } from "$lib/components/ui/radio";
  import CredentialField from "./CredentialField.svelte";
  import {
    getToolConfig,
    updateToolConfig,
    type ToolStatusDto,
  } from "$lib/stores/toolGovernance";

  interface Props {
    open: boolean;
    tool: ToolStatusDto | null;
    onclose: () => void;
  }

  let { open, tool, onclose }: Props = $props();

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
    backend: "auto",
    require_configured: false,
    brave_timeout_secs: 15,
    brave_max_results: 10,
    ddg_timeout_secs: 15,
    ddg_max_response_kb: 1024,
  };

  const DEFAULT_WEB_READ: WebReadFormState = {
    timeout_secs: 20,
    max_response_kb: 2048,
    ssrf_guard: true,
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
      require_configured: readBool(cfg, "require_configured", false),
      brave_timeout_secs: readNumber(brave, "timeout_secs", 15),
      brave_max_results: readNumber(brave, "max_results", 10),
      ddg_timeout_secs: readNumber(ddg, "timeout_secs", 15),
      ddg_max_response_kb: readNumber(ddg, "max_response_kb", 1024),
    };
  }

  function applyWebReadConfig(cfg: Record<string, unknown> | null): void {
    webRead = {
      timeout_secs: readNumber(cfg, "timeout_secs", 20),
      max_response_kb: readNumber(cfg, "max_response_kb", 2048),
      ssrf_guard: readBool(cfg, "ssrf_guard", true),
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
  <div class="flex h-full flex-col">
    <header class="flex items-center justify-between border-b border-border/40 px-5 py-4">
      <div class="space-y-0.5">
        <h2 class="text-lg font-semibold">
          Configurer {tool?.name ?? ""}
        </h2>
        {#if tool?.active_backend}
          <p class="text-xs text-muted-foreground">
            Backend actif : <code class="rounded bg-muted/40 px-1">{tool.active_backend}</code>
          </p>
        {/if}
      </div>
      <button
        type="button"
        aria-label="Fermer"
        onclick={onclose}
        class="rounded-md p-1.5 text-muted-foreground hover:bg-muted hover:text-foreground"
      >
        <X size={16} aria-hidden="true" />
      </button>
    </header>

    <div class="flex-1 overflow-y-auto px-5 py-5 space-y-6" data-testid="tool-config-drawer-body">
      {#if loading}
        <p class="text-sm text-muted-foreground">Chargement…</p>
      {:else if !tool}
        <p class="text-sm text-muted-foreground">Aucun outil sélectionné.</p>
      {:else if tool.name === "web_search"}
        <section class="space-y-3">
          <h3 class="text-sm font-medium">Backend préféré</h3>
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
              Auto (DDG par défaut, Brave si une clé est présente)
            </RadioItem>
            <RadioItem
              value="duckduckgo"
              checked={webSearch.backend === "duckduckgo"}
              onchange={(v) => (webSearch.backend = v as WebSearchBackend)}
            >
              DuckDuckGo uniquement
            </RadioItem>
            <RadioItem
              value="brave"
              checked={webSearch.backend === "brave"}
              onchange={(v) => (webSearch.backend = v as WebSearchBackend)}
            >
              Brave Search uniquement
            </RadioItem>
          </RadioGroup>
        </section>

        <section class="space-y-3">
          <h3 class="text-sm font-medium">Brave Search</h3>
          <CredentialField
            toolName="web_search"
            keyName="brave.api_key"
            label="Clé API Brave"
            configured={braveConfigured}
            canTest
            data-testid="brave-api-key"
          />
          <div class="grid grid-cols-2 gap-3">
            <label class="space-y-1 text-sm">
              <span class="text-muted-foreground">Timeout (s)</span>
              <input
                type="number"
                min="1"
                max="120"
                bind:value={webSearch.brave_timeout_secs}
                class="flex h-10 w-full rounded-md border border-border bg-background px-3 text-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                data-testid="brave-timeout"
              />
            </label>
            <label class="space-y-1 text-sm">
              <span class="text-muted-foreground">Résultats max (1–20)</span>
              <input
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
              <span class="text-muted-foreground">Timeout (s)</span>
              <input
                type="number"
                min="1"
                max="120"
                bind:value={webSearch.ddg_timeout_secs}
                class="flex h-10 w-full rounded-md border border-border bg-background px-3 text-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                data-testid="ddg-timeout"
              />
            </label>
            <label class="space-y-1 text-sm">
              <span class="text-muted-foreground">Taille max (Ko)</span>
              <input
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
              Exiger un backend configuré
              <span class="block text-xs text-muted-foreground">
                Erreur au démarrage si aucun backend valide.
              </span>
            </span>
          </label>
        </section>
      {:else if tool.name === "web_read"}
        <section class="space-y-3">
          <div class="grid grid-cols-2 gap-3">
            <label class="space-y-1 text-sm">
              <span class="text-muted-foreground">Timeout (s)</span>
              <input
                type="number"
                min="1"
                max="120"
                bind:value={webRead.timeout_secs}
                class="flex h-10 w-full rounded-md border border-border bg-background px-3 text-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                data-testid="web-read-timeout"
              />
            </label>
            <label class="space-y-1 text-sm">
              <span class="text-muted-foreground">Taille max (Ko)</span>
              <input
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
              Garde SSRF (rejeter les hôtes privés/loopback)
            </span>
          </label>
        </section>
      {:else}
        <p class="text-sm text-muted-foreground">
          Cet outil n'expose pas de configuration spécifique. Son activation se
          gère depuis la liste des outils.
        </p>
      {/if}

      {#if error}
        <p class="text-sm text-destructive" data-testid="tool-config-error">{error}</p>
      {/if}
    </div>

    <footer class="flex items-center justify-end gap-2 border-t border-border/40 px-5 py-3">
      <Button variant="ghost" onclick={onclose}>Annuler</Button>
      {#if hasConfigForm}
        <Button onclick={save} loading={saving} disabled={saving} data-testid="tool-config-save">
          Enregistrer
        </Button>
      {/if}
    </footer>
  </div>
</Sheet>
