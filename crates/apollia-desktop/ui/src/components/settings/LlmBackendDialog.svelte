<script lang="ts" module>
  import type { LlmBackendConfig } from "$lib/types";

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

  /** Pure validator — unit-testable. */
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
    if (isRemoteProvider(state.provider)) {
      if (state.endpoint.trim()) base.endpoint = state.endpoint.trim();
      if (state.apiKey.trim()) base.api_key = state.apiKey.trim();
    }
    if (Number.isFinite(state.timeoutSec) && state.timeoutSec > 0) {
      base.timeout_sec = state.timeoutSec;
    }
    return base;
  }
</script>

<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import Dialog from "$lib/components/ui/dialog/Dialog.svelte";
  import DialogFooter from "$lib/components/ui/dialog/DialogFooter.svelte";
  import { Button } from "$lib/components/ui/button";
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
      };
    }
    const cfg = (b.config_json ?? {}) as Record<string, unknown>;
    const endpoint = typeof cfg.endpoint === "string" ? cfg.endpoint : "";
    const apiKey = typeof cfg.api_key === "string" ? cfg.api_key : "";
    const timeout = typeof cfg.timeout_sec === "number" ? cfg.timeout_sec : 60;
    const rest = { ...cfg };
    delete rest.endpoint;
    delete rest.api_key;
    delete rest.timeout_sec;
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
    };
  }

  let form = $state<BackendFormState>(seedFrom(null));
  let advancedOpen = $state(false);
  let saving = $state(false);
  let testing = $state(false);
  let topLevelError = $state<string | null>(null);
  let testResult = $state<{ ok: boolean; message: string } | null>(null);

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
      lastBackendKey = key;
    }
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
      onsaved();
      onclose();
    } catch (err) {
      topLevelError = err instanceof Error ? err.message : String(err);
    } finally {
      saving = false;
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
      <div class="space-y-1">
        <label for="llm-name" class="text-xs font-medium text-muted-foreground">
          {$t("settings.llm_dialog.field_name")}
        </label>
        <input
          id="llm-name"
          type="text"
          placeholder="local-code"
          class={inputClass("name") + " font-mono"}
          bind:value={form.name}
          disabled={editing}
          data-testid="llm-dialog-name"
        />
        {#if labelFor("name")}
          <p class="text-xs text-destructive" data-testid="llm-dialog-name-error">{labelFor("name")}</p>
        {/if}
      </div>
      <div class="space-y-1">
        <label for="llm-provider" class="text-xs font-medium text-muted-foreground">
          {$t("settings.llm_dialog.field_provider")}
        </label>
        <select
          id="llm-provider"
          class={inputClass("provider") + " appearance-none"}
          bind:value={form.provider}
          data-testid="llm-dialog-provider"
        >
          <option value="llama-cpp">llama-cpp (local)</option>
          <option value="openai">OpenAI</option>
          <option value="anthropic">Anthropic</option>
          <option value="mistral">Mistral</option>
          <option value="ollama">Ollama</option>
        </select>
      </div>
    </div>

    {#if isRemoteProvider(form.provider)}
      <div class="space-y-1">
        <label for="llm-endpoint" class="text-xs font-medium text-muted-foreground">
          {$t("settings.llm_dialog.field_endpoint")}
        </label>
        <input
          id="llm-endpoint"
          type="url"
          placeholder="https://api.openai.com/v1"
          class={inputClass("endpoint") + " font-mono"}
          bind:value={form.endpoint}
          data-testid="llm-dialog-endpoint"
        />
        {#if labelFor("endpoint")}
          <p class="text-xs text-destructive" data-testid="llm-dialog-endpoint-error">{labelFor("endpoint")}</p>
        {/if}
      </div>
      <div class="space-y-1">
        <label for="llm-key" class="text-xs font-medium text-muted-foreground">
          {$t("settings.llm_dialog.field_api_key")}
        </label>
        <input
          id="llm-key"
          type="password"
          placeholder={editing ? $t("settings.llm_dialog.api_key_unchanged") : "sk-..."}
          class={inputClass("apiKey") + " font-mono"}
          bind:value={form.apiKey}
          data-testid="llm-dialog-apikey"
        />
        {#if labelFor("apiKey")}
          <p class="text-xs text-destructive" data-testid="llm-dialog-apikey-error">{labelFor("apiKey")}</p>
        {/if}
      </div>
    {/if}

    <div class="space-y-1">
      <label for="llm-model" class="text-xs font-medium text-muted-foreground">
        {$t("settings.llm_dialog.field_model")}
      </label>
      <input
        id="llm-model"
        type="text"
        placeholder={form.provider === "llama-cpp" ? "qwen3-0.6b-q8_0" : "gpt-4o-mini"}
        class={inputClass("model") + " font-mono"}
        bind:value={form.model}
        data-testid="llm-dialog-model"
      />
      {#if labelFor("model")}
        <p class="text-xs text-destructive" data-testid="llm-dialog-model-error">{labelFor("model")}</p>
      {/if}
    </div>

    <!-- Advanced collapsible -->
    <div class="rounded-md border border-border/60">
      <button
        type="button"
        class="flex w-full items-center justify-between px-3 py-2 text-xs font-medium text-muted-foreground hover:bg-muted/40"
        onclick={() => (advancedOpen = !advancedOpen)}
        aria-expanded={advancedOpen}
        data-testid="llm-dialog-advanced-toggle"
      >
        <span>{$t("settings.llm_dialog.advanced")}</span>
        <span aria-hidden="true">{advancedOpen ? "▾" : "▸"}</span>
      </button>
      {#if advancedOpen}
        <div class="space-y-3 border-t border-border/60 p-3">
          <div class="space-y-1">
            <label for="llm-timeout" class="text-xs font-medium text-muted-foreground">
              {$t("settings.llm_dialog.field_timeout")}
            </label>
            <input
              id="llm-timeout"
              type="number"
              min="1"
              max="600"
              class={inputClass("timeoutSec") + " font-mono"}
              bind:value={form.timeoutSec}
              data-testid="llm-dialog-timeout"
            />
            {#if labelFor("timeoutSec")}
              <p class="text-xs text-destructive">{labelFor("timeoutSec")}</p>
            {/if}
          </div>
          <div class="space-y-1">
            <label for="llm-extra" class="text-xs font-medium text-muted-foreground">
              {$t("settings.llm_dialog.field_extra_json")}
            </label>
            <textarea
              id="llm-extra"
              rows={3}
              placeholder={`{ "device": "metal" }`}
              class={"flex w-full rounded-md border bg-background px-3 py-1.5 text-sm font-mono focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/40 " +
                (errors.extraJson ? "border-destructive/60" : "border-border")}
              bind:value={form.extraJson}
              data-testid="llm-dialog-extra"
            ></textarea>
            {#if labelFor("extraJson")}
              <p class="text-xs text-destructive">{labelFor("extraJson")}</p>
            {/if}
          </div>
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
