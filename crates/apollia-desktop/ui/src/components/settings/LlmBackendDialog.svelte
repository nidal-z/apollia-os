<script lang="ts" module>
  import type { LlmBackendConfig } from "$lib/types";
  import { Input } from "$lib/components/ui/input";
  import { Select } from "$lib/components/ui/select";
  import { Textarea } from "$lib/components/ui/textarea";

  export type LlmProvider = LlmBackendConfig["provider"];

  export interface BackendFormState {
    name: string;
    provider: LlmProvider;
    endpoint: string;
    apiKey: string;
    model: string;
    timeoutSec: number;
    enabled: boolean;
    isDefault: boolean;
    extraJson: string;
    // Generation parameters (null = not set / use model defaults)
    temperature: number | null;
    topK: number | null;
    topP: number | null;
    repeatPenalty: number | null;
    contextSize: number | null;
  }

  export interface ValidationErrors {
    name?: string;
    provider?: string;
    endpoint?: string;
    apiKey?: string;
    model?: string;
    timeoutSec?: string;
    extraJson?: string;
  }

  const URL_RE = /^https?:\/\/[^\s]+$/i;
  const NAME_RE = /^[a-z0-9_-]+$/;

  /** Returns `true` when this provider needs a remote endpoint + api key. */
  export function isRemoteProvider(p: LlmProvider): boolean {
    return p === "openai" || p === "mistral" || p === "anthropic" || p === "ollama";
  }

  /** Pure validator - unit-testable. */
  export function validateForm(state: BackendFormState, editing: boolean): ValidationErrors {
    const errors: ValidationErrors = {};
    if (!state.name.trim()) {
      errors.name = "required";
    } else if (!NAME_RE.test(state.name)) {
      errors.name = "invalid_name";
    }
    if (!state.model.trim()) {
      errors.model = "required";
    }
    if (isRemoteProvider(state.provider)) {
      if (!state.endpoint.trim()) {
        errors.endpoint = "required";
      } else if (!URL_RE.test(state.endpoint.trim())) {
        errors.endpoint = "invalid_url";
      }
      // api_key optional for ollama; required for others
      if (state.provider !== "ollama" && !state.apiKey.trim() && !editing) {
        errors.apiKey = "required";
      }
    }
    if (!Number.isFinite(state.timeoutSec) || state.timeoutSec <= 0) {
      errors.timeoutSec = "invalid_timeout";
    }
    if (state.extraJson.trim().length > 0) {
      try {
        const parsed = JSON.parse(state.extraJson);
        if (typeof parsed !== "object" || Array.isArray(parsed) || parsed === null) {
          errors.extraJson = "invalid_json_object";
        }
      } catch {
        errors.extraJson = "invalid_json";
      }
    }
    return errors;
  }

  export function buildConfigJson(state: BackendFormState): Record<string, unknown> {
    let base: Record<string, unknown> = {};
    if (state.extraJson.trim()) {
      try {
        base = JSON.parse(state.extraJson) as Record<string, unknown>;
      } catch {
        base = {};
      }
    }
    // For llama-cpp, model_path in config_json must always match form.model.
    // This overwrites any stale model_path that may have survived in extraJson.
    if (state.provider === "llama-cpp" && state.model.trim()) {
      base.model_path = state.model.trim();
      delete base.model_paths;
    }
    if (isRemoteProvider(state.provider)) {
      // Persist the URL under the canonical `base_url` key (the one the router
      // reads). Drop the legacy `endpoint`/`api_url` aliases so an edited row
      // converges on a single key instead of keeping a stale duplicate.
      if (state.endpoint.trim()) base.base_url = state.endpoint.trim();
      delete base.endpoint;
      delete base.api_url;
      if (state.apiKey.trim()) base.api_key = state.apiKey.trim();
    }
    if (Number.isFinite(state.timeoutSec) && state.timeoutSec > 0) {
      base.timeout_sec = state.timeoutSec;
    }
    if (state.temperature !== null) base.temperature = state.temperature;
    if (state.topK !== null) base.top_k = state.topK;
    if (state.topP !== null) base.top_p = state.topP;
    if (state.repeatPenalty !== null) base.repeat_penalty = state.repeatPenalty;
    if (state.contextSize !== null) base.context_size = state.contextSize;
    return base;
  }
</script>

<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { open as openFilePicker } from "@tauri-apps/plugin-dialog";
  import { homeDir, join as pathJoin } from "@tauri-apps/api/path";
  import { t } from "svelte-i18n";
  import { FolderOpen, ScanSearch, Info, Check, Sparkles } from "lucide-svelte";
  import Dialog from "$lib/components/ui/dialog/Dialog.svelte";
  import DialogFooter from "$lib/components/ui/dialog/DialogFooter.svelte";
  import { Button } from "$lib/components/ui/button";
  import { FormField } from "$lib/components/ui/form-field";
  import SettingsToggle from "./SettingsToggle.svelte";
  import type { LlmPingResult } from "$lib/types";

  interface Props {
    open: boolean;
    backend?: LlmBackendConfig | null;
    onclose: () => void;
    onsaved: () => void;
  }

  let { open, backend = null, onclose, onsaved }: Props = $props();

  const editing = $derived(!!backend);

  function numOrNull(v: unknown): number | null {
    return typeof v === "number" && Number.isFinite(v) ? v : null;
  }

  function seedFrom(b: LlmBackendConfig | null): BackendFormState {
    if (!b) {
      return {
        name: "",
        provider: "llama-cpp",
        endpoint: "",
        apiKey: "",
        model: "",
        timeoutSec: 60,
        enabled: true,
        isDefault: false,
        extraJson: "",
        temperature: null,
        topK: null,
        topP: null,
        repeatPenalty: null,
        contextSize: null,
      };
    }
    const cfg = (b.config_json ?? {}) as Record<string, unknown>;
    // The URL reads from the canonical `base_url`, falling back to the legacy
    // `endpoint`/`api_url` aliases so backends persisted before the key
    // convergence still populate the field.
    const urlValue = [cfg.base_url, cfg.endpoint, cfg.api_url].find(
      (v): v is string => typeof v === "string" && v.length > 0,
    );
    const endpoint = urlValue ?? "";
    const apiKey = typeof cfg.api_key === "string" ? cfg.api_key : "";
    const timeout = typeof cfg.timeout_sec === "number" ? cfg.timeout_sec : 60;
    const rest = { ...cfg };
    delete rest.base_url;
    delete rest.endpoint;
    delete rest.api_url;
    delete rest.api_key;
    delete rest.timeout_sec;
    delete rest.temperature;
    delete rest.top_k;
    delete rest.top_p;
    delete rest.repeat_penalty;
    delete rest.context_size;
    // model_path / model_paths are managed via form.model - never round-trip via extraJson
    delete rest.model_path;
    delete rest.model_paths;
    return {
      name: b.name,
      provider: b.provider,
      endpoint,
      apiKey,
      model: b.model,
      timeoutSec: timeout,
      enabled: b.enabled,
      isDefault: b.is_default,
      extraJson: Object.keys(rest).length ? JSON.stringify(rest, null, 2) : "",
      temperature: numOrNull(cfg.temperature),
      topK: numOrNull(cfg.top_k),
      topP: numOrNull(cfg.top_p),
      repeatPenalty: numOrNull(cfg.repeat_penalty),
      contextSize: numOrNull(cfg.context_size),
    };
  }

  interface GgufModelInfo {
    path: string;
    filename: string;
    size_human: string;
    recommended: boolean;
  }

  interface HfModelCard {
    generation_config: {
      temperature?: number;
      top_p?: number;
      top_k?: number;
      repetition_penalty?: number;
      max_new_tokens?: number;
    } | null;
  }

  let form = $state<BackendFormState>(seedFrom(null));
  let advancedOpen = $state(false);
  let saving = $state(false);
  let testing = $state(false);
  let browsing = $state(false);
  let scanning = $state(false);
  let scannedModels = $state<GgufModelInfo[]>([]);
  let showScanDropdown = $state(false);
  let multipartHint = $state<{ part: number; total: number } | null>(null);
  let topLevelError = $state<string | null>(null);
  let testResult = $state<{ ok: boolean; message: string } | null>(null);
  let hfRepoId = $state("");
  let fetchingHfParams = $state(false);
  let hfFillResult = $state<{ ok: boolean; message: string } | null>(null);

  /**
   * Returns multi-part info if the filename matches the llama.cpp split pattern
   * `<name>-<NNNNN>-of-<TOTAL>.gguf`, otherwise null.
   */
  function detectMultipart(filePath: string): { part: number; total: number; firstPath: string } | null {
    const m = filePath.match(/^(.*?)-(\d{5})-of-(\d{5})\.gguf$/i);
    if (!m) return null;
    const part = parseInt(m[2], 10);
    const total = parseInt(m[3], 10);
    const firstPath = `${m[1]}-00001-of-${m[3]}.gguf`;
    return { part, total, firstPath };
  }

  async function handleBrowse(): Promise<void> {
    if (browsing) return;
    browsing = true;
    showScanDropdown = false;
    try {
      // Open the dialog directly in ~/.apollia/models so users don't need to
      // navigate to hidden directories manually.
      const home = await homeDir();
      const modelsDir = await pathJoin(home, ".apollia", "models");
      const selected = await openFilePicker({
        multiple: false,
        filters: [{ name: "GGUF Models", extensions: ["gguf"] }],
        title: $t("settings.llm_dialog.browse_title"),
        defaultPath: modelsDir,
      });
      if (!selected) return;
      const filePath = typeof selected === "string" ? selected : (selected as { path: string }).path;
      applySelectedPath(filePath);
    } finally {
      browsing = false;
    }
  }

  async function handleScan(): Promise<void> {
    if (scanning) return;
    scanning = true;
    showScanDropdown = false;
    try {
      const models = await invoke<GgufModelInfo[]>("scan_for_gguf_models");
      scannedModels = models;
      showScanDropdown = models.length > 0;
    } catch {
      scannedModels = [];
    } finally {
      scanning = false;
    }
  }

  function selectScannedModel(model: GgufModelInfo): void {
    applySelectedPath(model.path);
    showScanDropdown = false;
  }

  function applySelectedPath(filePath: string): void {
    const mp = detectMultipart(filePath);
    if (mp) {
      form.model = mp.firstPath;
      multipartHint = { part: mp.part, total: mp.total };
    } else {
      form.model = filePath;
      multipartHint = null;
    }
  }

  // Re-seed whenever we (re)open with a (possibly new) backend.
  let lastBackendKey = "";
  $effect(() => {
    if (!open) return;
    const key = backend ? `${backend.name}` : "__new__";
    if (key !== lastBackendKey) {
      form = seedFrom(backend);
      advancedOpen = false;
      testResult = null;
      topLevelError = null;
      multipartHint = null;
      scannedModels = [];
      showScanDropdown = false;
      hfRepoId = "";
      hfFillResult = null;
      lastBackendKey = key;
    }
  });

  // Clear multipart hint when provider changes to non-llama-cpp
  $effect(() => {
    if (form.provider !== "llama-cpp") multipartHint = null;
  });

  const errors = $derived(validateForm(form, editing));
  const hasErrors = $derived(Object.keys(errors).length > 0);

  async function handleTest() {
    testing = true;
    testResult = null;
    topLevelError = null;
    try {
      if (!editing) {
        // For new backends, we can only validate locally before save.
        if (hasErrors) {
          testResult = { ok: false, message: $t("settings.llm_dialog.test_invalid") };
        } else {
          testResult = { ok: true, message: $t("settings.llm_dialog.test_local_ok") };
        }
        return;
      }
      const res = (await invoke("ping_llm_backend", { name: form.name })) as LlmPingResult;
      if (res.available) {
        testResult = {
          ok: true,
          message: $t("settings.llm_dialog.test_ok", {
            values: { ms: String(res.latency_ms ?? 0) },
          }),
        };
      } else {
        testResult = {
          ok: false,
          message: res.error ?? $t("settings.llm_dialog.test_failed"),
        };
      }
    } catch (err) {
      testResult = { ok: false, message: err instanceof Error ? err.message : String(err) };
    } finally {
      testing = false;
    }
  }

  async function handleSave() {
    if (hasErrors) return;
    saving = true;
    topLevelError = null;
    try {
      const config_json = buildConfigJson(form);
      if (editing && backend) {
        await invoke("update_llm_backend", {
          name: backend.name,
          payload: {
            provider: form.provider,
            model: form.model,
            config_json,
            enabled: form.enabled,
            is_default: form.isDefault,
          },
        });
      } else {
        await invoke("create_llm_backend", {
          payload: {
            name: form.name,
            provider: form.provider,
            model: form.model,
            config_json,
            enabled: form.enabled,
            is_default: form.isDefault,
          },
        });
      }
      // Rebuild the in-memory LlmRouter from the updated SQLite repository so
      // agents immediately use the new backend without restarting the app.
      await invoke("reload_llm_from_db").catch((e: unknown) => {
        console.warn("[LlmBackendDialog] reload_llm_from_db failed:", e);
      });
      onsaved();
      onclose();
    } catch (err) {
      topLevelError = err instanceof Error ? err.message : String(err);
    } finally {
      saving = false;
    }
  }

  async function fetchHfParams(): Promise<void> {
    const repoId = hfRepoId.trim();
    if (!repoId || fetchingHfParams) return;
    fetchingHfParams = true;
    hfFillResult = null;
    try {
      const card = await invoke<HfModelCard>("get_hf_model", { repoId, hfToken: null });
      const gc = card.generation_config;
      if (!gc) {
        hfFillResult = { ok: false, message: $t("settings.llm_dialog.hf_no_config") };
        return;
      }
      if (gc.temperature !== undefined) form.temperature = gc.temperature;
      if (gc.top_k !== undefined) form.topK = gc.top_k;
      if (gc.top_p !== undefined) form.topP = gc.top_p;
      if (gc.repetition_penalty !== undefined) form.repeatPenalty = gc.repetition_penalty;
      if (gc.max_new_tokens !== undefined) form.contextSize = gc.max_new_tokens;
      hfFillResult = { ok: true, message: $t("settings.llm_dialog.hf_filled") };
    } catch (err) {
      hfFillResult = {
        ok: false,
        message: err instanceof Error ? err.message : String(err),
      };
    } finally {
      fetchingHfParams = false;
    }
  }

  function labelFor(key: keyof ValidationErrors): string | null {
    const code = errors[key];
    if (!code) return null;
    return $t(`settings.llm_dialog.err_${code}`);
  }

  function inputClass(field: keyof ValidationErrors): string {
    const base =
      "flex h-9 w-full rounded-md border bg-background px-3 py-1.5 text-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/40";
    return errors[field] ? `${base} border-destructive/60` : `${base} border-border`;
  }
</script>

<Dialog
  {open}
  {onclose}
  size="lg"
  title={editing
    ? $t("settings.llm_dialog.title_edit", { values: { name: backend?.name ?? "" } })
    : $t("settings.llm_dialog.title_add")}
  data-testid="llm-backend-dialog"
>
  <div class="space-y-4">
    {#if topLevelError}
      <div
        class="rounded-md border border-destructive/30 bg-destructive/5 px-3 py-2 text-sm text-destructive"
        data-testid="llm-dialog-error"
      >
        {topLevelError}
      </div>
    {/if}

    <div class="grid grid-cols-1 sm:grid-cols-2 gap-3">
      <FormField
        id="llm-name"
        label={$t("settings.llm_dialog.field_name")}
        error={labelFor("name") || undefined}
      >
        <Input
          id="llm-name"
          type="text"
          placeholder="local-code"
          class={inputClass("name") + " font-mono"}
          bind:value={form.name}
          disabled={editing}
          data-testid="llm-dialog-name"
         />
      </FormField>
      <FormField id="llm-provider" label={$t("settings.llm_dialog.field_provider")}>
        <Select
          id="llm-provider"
          class={inputClass("provider") + " appearance-none"}
          bind:value={form.provider}
          data-testid="llm-dialog-provider"
        >
          <option value="llama-cpp">llama-cpp ({$t("settings.llm_backend.provider_local")})</option>
          <option value="openai">OpenAI</option>
          <option value="anthropic">Anthropic</option>
          <option value="mistral">Mistral</option>
          <option value="ollama">Ollama</option>
        </Select>
      </FormField>
    </div>

    {#if isRemoteProvider(form.provider)}
      <FormField
        id="llm-endpoint"
        label={$t("settings.llm_dialog.field_endpoint")}
        error={labelFor("endpoint") || undefined}
      >
        <Input
          id="llm-endpoint"
          type="url"
          placeholder="https://api.openai.com/v1"
          class={inputClass("endpoint") + " font-mono"}
          bind:value={form.endpoint}
          data-testid="llm-dialog-endpoint"
         />
      </FormField>
      <FormField
        id="llm-key"
        label={$t("settings.llm_dialog.field_api_key")}
        error={labelFor("apiKey") || undefined}
      >
        <Input
          id="llm-key"
          type="password"
          placeholder={editing ? $t("settings.llm_dialog.api_key_unchanged") : "sk-..."}
          class={inputClass("apiKey") + " font-mono"}
          bind:value={form.apiKey}
          data-testid="llm-dialog-apikey"
         />
      </FormField>
    {/if}

    <FormField
      id="llm-model"
      label={form.provider === "llama-cpp"
        ? $t("settings.llm_dialog.field_model_path")
        : $t("settings.llm_dialog.field_model")}
    >
      {#if form.provider === "llama-cpp"}
        <div class="flex gap-1.5">
          <Input
            id="llm-model"
            type="text"
            placeholder="/path/to/model.gguf"
            class={inputClass("model") + " font-mono flex-1 min-w-0"}
            bind:value={form.model}
            data-testid="llm-dialog-model"
           />
          <Button variant="ghost" size="sm"
            type="button"
            class="inline-flex shrink-0 items-center gap-1.5 rounded-md border border-border bg-background px-2.5 py-1 text-xs text-muted-foreground hover:bg-muted/60 hover:text-foreground disabled:opacity-50"
            onclick={handleBrowse}
            disabled={browsing}
            data-testid="llm-dialog-browse"
          >
            <FolderOpen size={13} strokeWidth={1.75} />
            {browsing ? "…" : $t("settings.llm_dialog.browse")}
          </Button>
          <Button variant="ghost" size="sm"
            type="button"
            class="inline-flex shrink-0 items-center gap-1.5 rounded-md border border-border bg-background px-2.5 py-1 text-xs text-muted-foreground hover:bg-muted/60 hover:text-foreground disabled:opacity-50"
            onclick={handleScan}
            disabled={scanning}
            data-testid="llm-dialog-scan"
          >
            <ScanSearch size={13} strokeWidth={1.75} />
            {scanning ? "…" : $t("settings.llm_dialog.scan")}
          </Button>
        </div>
        {#if showScanDropdown && scannedModels.length > 0}
          <div
            class="rounded-md border border-border bg-background shadow-md"
            data-testid="llm-dialog-scan-results"
          >
            {#each scannedModels as model (model.path)}
              <Button variant="ghost" size="sm"
                type="button"
                class="flex w-full items-center gap-2 px-3 py-2 text-left text-xs hover:bg-muted/60 first:rounded-t-md last:rounded-b-md"
                onclick={() => selectScannedModel(model)}
                data-testid="llm-dialog-scan-result-row"
              >
                <span class="flex-1 truncate font-mono text-foreground">{model.filename}</span>
                <span class="shrink-0 text-muted-foreground">{model.size_human}</span>
                {#if model.recommended}
                  <span class="shrink-0 rounded-full bg-primary/10 px-1.5 py-0.5 text-[10px] font-semibold text-primary">
                    {$t("settings.llm_dialog.recommended")}
                  </span>
                {/if}
                {#if form.model === model.path}
                  <Check size={11} strokeWidth={2.5} class="shrink-0 text-primary" />
                {/if}
              </Button>
            {/each}
          </div>
        {:else if showScanDropdown && scannedModels.length === 0}
          <p class="text-xs text-muted-foreground" data-testid="llm-dialog-scan-empty">
            {$t("settings.llm_dialog.scan_empty")}
          </p>
        {/if}
        {#if multipartHint}
          <div
            class="flex items-start gap-1.5 rounded-md border border-blue-500/20 bg-blue-500/5 px-2.5 py-1.5 text-xs text-blue-600 dark:text-blue-400"
            data-testid="llm-dialog-multipart-hint"
          >
            <Info size={12} strokeWidth={2} class="mt-0.5 shrink-0" />
            <span>
              {multipartHint.part === 1
                ? $t("settings.llm_dialog.multipart_first", { values: { total: String(multipartHint.total) } })
                : $t("settings.llm_dialog.multipart_corrected", { values: { part: String(multipartHint.part), total: String(multipartHint.total) } })}
            </span>
          </div>
        {/if}
      {:else}
        <Input
          id="llm-model"
          type="text"
          placeholder="gpt-4o-mini"
          class={inputClass("model") + " font-mono"}
          bind:value={form.model}
          data-testid="llm-dialog-model"
         />
      {/if}
      {#if labelFor("model")}
        <p class="text-xs text-destructive" data-testid="llm-dialog-model-error">{labelFor("model")}</p>
      {/if}
    </FormField>

    <!-- Advanced collapsible -->
    <div class="rounded-md border border-border/60">
      <Button variant="ghost" size="sm"
        type="button"
        class="flex w-full items-center justify-between px-3 py-2 text-xs font-medium text-muted-foreground hover:bg-muted/40"
        onclick={() => (advancedOpen = !advancedOpen)}
        aria-expanded={advancedOpen}
        data-testid="llm-dialog-advanced-toggle"
      >
        <span>{$t("settings.llm_dialog.advanced")}</span>
        <span aria-hidden="true">{advancedOpen ? "▾" : "▸"}</span>
      </Button>
      {#if advancedOpen}
        <div class="space-y-4 border-t border-border/60 p-3">
          <!-- Generation parameters -->
          <div class="space-y-2">
            <p class="text-xs font-semibold text-muted-foreground uppercase tracking-wide">
              {$t("settings.llm_dialog.section_gen_params")}
            </p>
            <div class="grid grid-cols-2 gap-2 sm:grid-cols-3">
              <FormField
                id="llm-temperature"
                label={$t("settings.llm_dialog.field_temperature")}
                labelClass="font-normal"
              >
                <Input
                  id="llm-temperature"
                  type="number"
                  min="0"
                  max="2"
                  step="0.05"
                  placeholder="-"
                  class="flex h-9 w-full rounded-md border border-border bg-background px-3 py-1.5 text-sm font-mono focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/40"
                  value={form.temperature ?? ""}
                  oninput={(e) => {
                    const v = (e.currentTarget as HTMLInputElement).valueAsNumber;
                    form.temperature = Number.isFinite(v) ? v : null;
                  }}
                  data-testid="llm-dialog-temperature"
                />
              </FormField>
              <FormField
                id="llm-top-k"
                label={$t("settings.llm_dialog.field_top_k")}
                labelClass="font-normal"
              >
                <Input
                  id="llm-top-k"
                  type="number"
                  min="0"
                  step="1"
                  placeholder="-"
                  class="flex h-9 w-full rounded-md border border-border bg-background px-3 py-1.5 text-sm font-mono focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/40"
                  value={form.topK ?? ""}
                  oninput={(e) => {
                    const v = (e.currentTarget as HTMLInputElement).valueAsNumber;
                    form.topK = Number.isFinite(v) ? Math.round(v) : null;
                  }}
                  data-testid="llm-dialog-top-k"
                />
              </FormField>
              <FormField
                id="llm-top-p"
                label={$t("settings.llm_dialog.field_top_p")}
                labelClass="font-normal"
              >
                <Input
                  id="llm-top-p"
                  type="number"
                  min="0"
                  max="1"
                  step="0.05"
                  placeholder="-"
                  class="flex h-9 w-full rounded-md border border-border bg-background px-3 py-1.5 text-sm font-mono focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/40"
                  value={form.topP ?? ""}
                  oninput={(e) => {
                    const v = (e.currentTarget as HTMLInputElement).valueAsNumber;
                    form.topP = Number.isFinite(v) ? v : null;
                  }}
                  data-testid="llm-dialog-top-p"
                />
              </FormField>
              <FormField
                id="llm-repeat-penalty"
                label={$t("settings.llm_dialog.field_repeat_penalty")}
                labelClass="font-normal"
              >
                <Input
                  id="llm-repeat-penalty"
                  type="number"
                  min="0"
                  max="2"
                  step="0.05"
                  placeholder="-"
                  class="flex h-9 w-full rounded-md border border-border bg-background px-3 py-1.5 text-sm font-mono focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/40"
                  value={form.repeatPenalty ?? ""}
                  oninput={(e) => {
                    const v = (e.currentTarget as HTMLInputElement).valueAsNumber;
                    form.repeatPenalty = Number.isFinite(v) ? v : null;
                  }}
                  data-testid="llm-dialog-repeat-penalty"
                />
              </FormField>
              <FormField
                id="llm-context-size"
                label={$t("settings.llm_dialog.field_context_size")}
                labelClass="font-normal"
              >
                <Input
                  id="llm-context-size"
                  type="number"
                  min="512"
                  step="512"
                  placeholder="-"
                  class="flex h-9 w-full rounded-md border border-border bg-background px-3 py-1.5 text-sm font-mono focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/40"
                  value={form.contextSize ?? ""}
                  oninput={(e) => {
                    const v = (e.currentTarget as HTMLInputElement).valueAsNumber;
                    form.contextSize = Number.isFinite(v) ? Math.round(v) : null;
                  }}
                  data-testid="llm-dialog-context-size"
                />
              </FormField>
            </div>
            <p class="text-[11px] text-muted-foreground/70">
              {$t("settings.llm_dialog.gen_params_hint")}
            </p>
          </div>

          <!-- HF auto-fill (llama-cpp only) -->
          {#if form.provider === "llama-cpp"}
            <div class="space-y-2 rounded-md border border-dashed border-border/60 p-2.5">
              <p class="text-xs font-semibold text-muted-foreground uppercase tracking-wide">
                {$t("settings.llm_dialog.hf_autofill_title")}
              </p>
              <div class="flex gap-1.5">
                <Input
                  type="text"
                  placeholder="Qwen/Qwen3-30B-A3B-GGUF"
                  class="flex h-9 flex-1 min-w-0 rounded-md border border-border bg-background px-3 py-1.5 text-sm font-mono focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/40"
                  bind:value={hfRepoId}
                  onkeydown={(e) => { if (e.key === "Enter") void fetchHfParams(); }}
                  data-testid="llm-dialog-hf-repo"
                />
                <Button variant="ghost" size="sm"
                  type="button"
                  class="inline-flex shrink-0 items-center gap-1.5 rounded-md border border-border bg-background px-2.5 py-1 text-xs text-muted-foreground hover:bg-muted/60 hover:text-foreground disabled:opacity-50"
                  onclick={() => void fetchHfParams()}
                  disabled={fetchingHfParams || !hfRepoId.trim()}
                  data-testid="llm-dialog-hf-fetch"
                >
                  {#if fetchingHfParams}
                    <span class="animate-spin text-base leading-none">⟳</span>
                  {:else}
                    <Sparkles size={13} strokeWidth={1.75} />
                  {/if}
                  {fetchingHfParams ? "…" : $t("settings.llm_dialog.hf_autofill_btn")}
                </Button>
              </div>
              {#if hfFillResult}
                <p class="text-xs {hfFillResult.ok ? 'text-green-600 dark:text-green-400' : 'text-destructive'}">
                  {hfFillResult.message}
                </p>
              {/if}
            </div>
          {/if}

          <!-- Timeout -->
          <FormField
            id="llm-timeout"
            label={$t("settings.llm_dialog.field_timeout")}
            error={labelFor("timeoutSec") || undefined}
          >
            <Input
              id="llm-timeout"
              type="number"
              min="1"
              max="600"
              class={inputClass("timeoutSec") + " font-mono"}
              bind:value={form.timeoutSec}
              data-testid="llm-dialog-timeout"
             />
          </FormField>

          <!-- Extra JSON -->
          <FormField
            id="llm-extra"
            label={$t("settings.llm_dialog.field_extra_json")}
            error={labelFor("extraJson") || undefined}
          >
            <Textarea
              id="llm-extra"
              rows={3}
              placeholder={`{ "device": "metal" }`}
              class={"flex w-full rounded-md border bg-background px-3 py-1.5 text-sm font-mono focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/40 " +
                (errors.extraJson ? "border-destructive/60" : "border-border")}
              bind:value={form.extraJson}
              data-testid="llm-dialog-extra"
            ></Textarea>
          </FormField>

          <SettingsToggle
            label={$t("settings.llm_backend_enabled")}
            bind:checked={form.enabled}
            data-testid="llm-dialog-enabled"
          />
          <SettingsToggle
            label={$t("settings.llm_backend_default")}
            bind:checked={form.isDefault}
            data-testid="llm-dialog-default"
          />
        </div>
      {/if}
    </div>

    {#if testResult}
      <div
        class="rounded-md border px-3 py-2 text-sm {testResult.ok
          ? 'border-success/30 bg-success/5 text-success-foreground'
          : 'border-destructive/30 bg-destructive/5 text-destructive'}"
        data-testid="llm-dialog-test-result"
      >
        {testResult.message}
      </div>
    {/if}
  </div>

  <DialogFooter>
    <Button variant="outline" onclick={onclose} data-testid="llm-dialog-cancel">
      {$t("common.cancel")}
    </Button>
    <Button
      variant="outline"
      onclick={handleTest}
      disabled={testing || (hasErrors && !editing)}
      data-testid="llm-dialog-test"
    >
      {testing ? $t("settings.llm_dialog.testing") : $t("settings.llm_dialog.test_connection")}
    </Button>
    <Button
      onclick={handleSave}
      disabled={saving || hasErrors}
      data-testid="llm-dialog-save"
    >
      {saving ? $t("common.saving") : $t("common.save")}
    </Button>
  </DialogFooter>
</Dialog>
