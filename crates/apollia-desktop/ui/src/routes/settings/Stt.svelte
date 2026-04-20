<script lang="ts" context="module">
  export const meta = {
    title: "settings.nav.stt",
    icon: "mic",
    group: "settings.nav.cluster_ai",
    cluster: "ai",
  } as const;
</script>

<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { t } from "svelte-i18n";
  import { Button } from "$lib/components/ui/button";
  import SettingSectionSkeleton from "../../components/settings/SettingSectionSkeleton.svelte";
  import { refreshSttStatus, sttStatus } from "$lib/stores/stt";
  import { sttModelsStore, sttConfigStore, settingsLoaders } from "$lib/stores/settings";
  import type { SttConfigView } from "$lib/types";

  // Local editable copy, synced from the shared store on load.
  let sttConfig = $state<SttConfigView | null>(null);
  let sttConfigSaved = $state(false);
  let sttSaving = $state(false);
  let sttSaveError = $state<string | null>(null);

  $effect(() => {
    const loaded = $sttConfigStore.data;
    if (loaded && !sttConfig) sttConfig = { ...loaded };
  });

  async function save() {
    if (!sttConfig) return;
    sttSaving = true;
    sttSaveError = null;
    sttConfigSaved = false;
    try {
      await invoke("update_stt_config", { config: sttConfig });
      sttConfigSaved = true;
    } catch (err) {
      sttSaveError = err instanceof Error ? err.message : String(err);
    } finally {
      sttSaving = false;
    }
  }

  // ─── Hotkey capture ─────────────────────────────────
  let hotkeyRecording = $state(false);
  let hotkeyPreview = $state("");
  let _activeMods = new Set<string>();

  const MOD_MAP: Record<string, string> = {
    Control: "ctrl",
    Shift: "shift",
    Alt: "alt",
    Meta: "meta",
  };
  const SKIP_KEYS = new Set(["CapsLock", "Dead", "Unidentified", "Process"]);

  function normalizeKey(code: string, fallbackKey: string): string {
    if (code.startsWith("Key")) return code.slice(3).toLowerCase();
    if (code.startsWith("Digit")) return code.slice(5);
    if (code.startsWith("Numpad")) return "numpad" + code.slice(6).toLowerCase();
    if (/^F\d+$/.test(code)) return code.toLowerCase();
    const named: Record<string, string> = {
      Space: "space", Enter: "enter", Backspace: "backspace", Tab: "tab",
      Delete: "delete", Insert: "insert", Escape: "escape",
      ArrowUp: "up", ArrowDown: "down", ArrowLeft: "left", ArrowRight: "right",
      Home: "home", End: "end", PageUp: "pageup", PageDown: "pagedown",
      Minus: "-", Equal: "=", BracketLeft: "[", BracketRight: "]",
      Semicolon: ";", Quote: "'", Backquote: "`", Comma: ",", Period: ".",
      Slash: "/", Backslash: "\\", Pause: "pause", PrintScreen: "print",
    };
    return named[code] ?? fallbackKey.toLowerCase();
  }

  function stopHotkeyCapture() {
    hotkeyRecording = false;
    hotkeyPreview = "";
    _activeMods.clear();
    document.removeEventListener("keydown", _onHotkeyKeydown, true);
    document.removeEventListener("keyup", _onHotkeyKeyup, true);
    document.removeEventListener("click", _onHotkeyOutside, true);
  }

  function _buildComboFromEvent(event: KeyboardEvent): string {
    const parts: string[] = [];
    if (_activeMods.has("ctrl") || event.ctrlKey) parts.push("ctrl");
    if (_activeMods.has("shift") || event.shiftKey) parts.push("shift");
    if (_activeMods.has("alt") || event.altKey) parts.push("alt");
    if (_activeMods.has("meta") || event.metaKey) parts.push("meta");
    parts.push(normalizeKey(event.code, event.key));
    return parts.join("+");
  }

  function _onHotkeyKeydown(event: KeyboardEvent) {
    event.preventDefault();
    event.stopPropagation();
    event.stopImmediatePropagation();
    if (event.key === "Escape") { stopHotkeyCapture(); return; }
    const mod = MOD_MAP[event.key];
    if (mod) {
      _activeMods.add(mod);
      const held = [];
      if (_activeMods.has("ctrl")) held.push("Ctrl");
      if (_activeMods.has("shift")) held.push("Shift");
      if (_activeMods.has("alt")) held.push("Alt");
      if (_activeMods.has("meta")) held.push("Cmd");
      hotkeyPreview = held.join("+") + (held.length ? " +" : "");
      return;
    }
    if (SKIP_KEYS.has(event.key)) return;
    const combo = _buildComboFromEvent(event);
    if (sttConfig) sttConfig.hotkey = combo;
    stopHotkeyCapture();
  }

  function _onHotkeyKeyup(event: KeyboardEvent) {
    event.preventDefault();
    event.stopPropagation();
    event.stopImmediatePropagation();
    const mod = MOD_MAP[event.key];
    if (!mod) return;
    const parts: string[] = [];
    if (_activeMods.has("ctrl")) parts.push("ctrl");
    if (_activeMods.has("shift")) parts.push("shift");
    if (_activeMods.has("alt")) parts.push("alt");
    if (_activeMods.has("meta")) parts.push("meta");
    _activeMods.delete(mod);
    if (_activeMods.size === 0) {
      if (parts.length > 0 && sttConfig) sttConfig.hotkey = parts.join("+");
      stopHotkeyCapture();
      return;
    }
    const held = [];
    if (_activeMods.has("ctrl")) held.push("Ctrl");
    if (_activeMods.has("shift")) held.push("Shift");
    if (_activeMods.has("alt")) held.push("Alt");
    if (_activeMods.has("meta")) held.push("Cmd");
    hotkeyPreview = held.join("+") + " +";
  }

  function _onHotkeyOutside(event: MouseEvent) {
    const btn = document.getElementById("stt-hotkey-btn");
    if (btn && !btn.contains(event.target as Node)) stopHotkeyCapture();
  }

  function startHotkeyCapture() {
    if (hotkeyRecording) return;
    _activeMods.clear();
    hotkeyPreview = "";
    hotkeyRecording = true;
    document.addEventListener("keydown", _onHotkeyKeydown, true);
    document.addEventListener("keyup", _onHotkeyKeyup, true);
    document.addEventListener("click", _onHotkeyOutside, true);
  }

  onMount(() => {
    void settingsLoaders.sttConfig();
    void settingsLoaders.sttModels();
    refreshSttStatus().catch(() => {});
  });
</script>

{#if $sttConfigStore.loading && !sttConfig}
  <SettingSectionSkeleton rows={2} />
{:else if $sttConfigStore.error}
  <div class="rounded-md border border-destructive/30 bg-destructive/5 px-4 py-2 text-sm text-destructive">
    {$t('settings.stt_error')}: {$sttConfigStore.error}
  </div>
{:else}
  <section class="space-y-5" data-testid="stt-section">
    <p class="text-sm text-muted-foreground">{$t('settings.stt_subtitle')}</p>

    {#if sttConfig}
      <div class="glass-card glass-border rounded-lg p-4" data-testid="stt-config-form">
        <h3 class="mb-4 text-sm font-medium uppercase tracking-wider text-muted-foreground">{$t('settings.stt_config_section')}</h3>

        <div class="space-y-4">
          <label class="flex cursor-pointer items-center justify-between" data-testid="stt-enable-toggle">
            <span class="text-sm text-foreground">{$t('settings.stt_enable_engine')}</span>
            <button
              type="button"
              role="switch"
              aria-checked={sttConfig.enabled}
              aria-label={$t('settings.stt_enable_engine')}
              onclick={() => { if (sttConfig) sttConfig.enabled = !sttConfig.enabled; }}
              class="relative inline-flex h-6 w-11 items-center rounded-full transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring {sttConfig.enabled ? 'bg-primary' : 'bg-muted'}"
            >
              <span class="inline-block h-4 w-4 rounded-full bg-white shadow-sm transition-transform {sttConfig.enabled ? 'translate-x-6' : 'translate-x-1'}"></span>
            </button>
          </label>

          <div class="space-y-1.5">
            <label class="text-sm text-muted-foreground" for="stt-model-select">{$t('settings.stt_select_model')}</label>
            {#if ($sttModelsStore.data ?? []).length > 0}
              <select
                id="stt-model-select"
                bind:value={sttConfig.model_path}
                class="flex h-9 w-full appearance-none rounded-md border border-border bg-background px-3 py-1.5 text-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/40"
                data-testid="stt-model-select"
              >
                {#each ($sttModelsStore.data ?? []) as model (model.name)}
                  <option value="~/.apollia/models/{model.name}">
                    {model.name} ({model.size_mb.toFixed(0)} Mo{model.language ? ` · ${model.language}` : ''})
                  </option>
                {/each}
              </select>
            {:else}
              <p class="rounded-md border border-border/50 px-3 py-2 text-sm text-muted-foreground">
                {$t('settings.stt_no_models')}
              </p>
            {/if}
          </div>

          <div class="space-y-1.5">
            <label class="text-sm text-muted-foreground" for="stt-language">{$t('settings.stt_language')}</label>
            <input
              id="stt-language"
              type="text"
              placeholder={$t('settings.stt_language_auto')}
              bind:value={sttConfig.language}
              class="flex h-9 w-full rounded-md border border-border bg-background px-3 py-1.5 text-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/40"
              data-testid="stt-language-input"
            />
          </div>

          <div class="space-y-1.5">
            <label class="text-sm text-muted-foreground" for="stt-hotkey-btn">{$t('settings.stt_hotkey')}</label>
            <button
              id="stt-hotkey-btn"
              type="button"
              aria-label={hotkeyRecording ? $t('settings.stt_hotkey_recording') : sttConfig.hotkey || $t('settings.stt_hotkey_placeholder')}
              class="flex h-9 w-full cursor-pointer items-center justify-between rounded-md border px-3 py-1.5 text-sm transition-colors {hotkeyRecording ? 'border-primary bg-primary/5 text-primary ring-2 ring-primary/30' : 'border-border bg-background text-foreground hover:border-border/80'}"
              data-testid="stt-hotkey-input"
              onclick={startHotkeyCapture}
            >
              <span class="font-mono">
                {#if hotkeyRecording}
                  {#if hotkeyPreview}
                    <span>{hotkeyPreview}</span>
                  {:else}
                    <span class="animate-pulse">{$t('settings.stt_hotkey_recording')}</span>
                  {/if}
                {:else}
                  {sttConfig.hotkey || $t('settings.stt_hotkey_placeholder')}
                {/if}
              </span>
            </button>
            {#if hotkeyRecording}
              <p class="text-xs text-primary/80">{$t('settings.stt_hotkey_hint')}</p>
            {/if}
          </div>

          <div class="grid grid-cols-1 sm:grid-cols-2 gap-4">
            <div class="space-y-1.5">
              <label class="text-sm text-muted-foreground" for="stt-trigger">{$t('settings.stt_trigger_mode')}</label>
              <select
                id="stt-trigger"
                bind:value={sttConfig.trigger_mode}
                class="flex h-9 w-full appearance-none rounded-md border border-border bg-background px-3 py-1.5 text-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/40"
              >
                <option value="toggle">{$t('settings.stt_trigger_toggle')}</option>
                <option value="push-to-talk">{$t('settings.stt_trigger_push')}</option>
              </select>
            </div>
            <div class="space-y-1.5">
              <label class="text-sm text-muted-foreground" for="stt-clipboard">{$t('settings.stt_clipboard_mode')}</label>
              <select
                id="stt-clipboard"
                bind:value={sttConfig.clipboard_mode}
                class="flex h-9 w-full appearance-none rounded-md border border-border bg-background px-3 py-1.5 text-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/40"
              >
                <option value="paste">{$t('settings.stt_clipboard_paste')}</option>
                <option value="clipboard">{$t('settings.stt_clipboard_clipboard')}</option>
              </select>
            </div>
          </div>

          <div class="grid grid-cols-1 sm:grid-cols-2 gap-4">
            <div class="space-y-1.5">
              <label class="text-sm text-muted-foreground" for="stt-max-rec">{$t('settings.stt_max_recording')}</label>
              <div class="relative">
                <input
                  id="stt-max-rec"
                  type="number"
                  min="5"
                  max="300"
                  bind:value={sttConfig.max_recording_sec}
                  class="flex h-9 w-full rounded-md border border-border bg-background px-3 py-1.5 pr-8 text-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/40"
                />
                <span class="pointer-events-none absolute inset-y-0 right-2.5 flex items-center text-xs text-muted-foreground">s</span>
              </div>
            </div>
            <div class="space-y-1.5">
              <label class="text-sm text-muted-foreground" for="stt-silence">{$t('settings.stt_silence_threshold')}</label>
              <div class="relative">
                <input
                  id="stt-silence"
                  type="number"
                  min="-80"
                  max="0"
                  step="1"
                  bind:value={sttConfig.silence_threshold_db}
                  class="flex h-9 w-full rounded-md border border-border bg-background px-3 py-1.5 pr-10 text-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/40"
                />
                <span class="pointer-events-none absolute inset-y-0 right-2.5 flex items-center text-xs text-muted-foreground">dB</span>
              </div>
            </div>
          </div>

          <label class="flex cursor-pointer items-center justify-between">
            <span class="text-sm text-muted-foreground">{$t('settings.stt_clipboard_restore')}</span>
            <button
              type="button"
              role="switch"
              aria-checked={sttConfig.clipboard_restore}
              aria-label={$t('settings.stt_clipboard_restore')}
              onclick={() => { if (sttConfig) sttConfig.clipboard_restore = !sttConfig.clipboard_restore; }}
              class="relative inline-flex h-6 w-11 items-center rounded-full transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring {sttConfig.clipboard_restore ? 'bg-primary' : 'bg-muted'}"
            >
              <span class="inline-block h-4 w-4 rounded-full bg-white shadow-sm transition-transform {sttConfig.clipboard_restore ? 'translate-x-6' : 'translate-x-1'}"></span>
            </button>
          </label>
        </div>

        <div class="mt-5 flex items-center gap-3">
          <Button onclick={save} disabled={sttSaving} data-testid="stt-save-btn">
            {sttSaving ? $t('settings.stt_saving') : $t('settings.stt_save')}
          </Button>
          {#if sttSaveError}
            <span class="text-sm text-destructive">{$t('settings.stt_save_error')}: {sttSaveError}</span>
          {/if}
        </div>
      </div>
    {/if}

    {#if sttConfigSaved}
      <div class="flex items-start gap-3 rounded-md border border-amber-500/30 bg-amber-500/10 px-4 py-3 text-sm text-amber-700 dark:text-amber-400" data-testid="stt-restart-notice" role="alert">
        {$t('settings.stt_restart_notice')}
      </div>
    {/if}

    <div class="glass-card glass-border rounded-lg p-4" data-testid="stt-engine-status">
      <h3 class="mb-3 text-sm font-medium uppercase tracking-wider text-muted-foreground">{$t('settings.stt_engine_status_section')}</h3>
      <div class="space-y-2">
        <div class="grid grid-cols-1 sm:grid-cols-2 gap-2">
          <span class="text-sm text-muted-foreground">{$t('settings.stt_engine_status')}</span>
          <span class="text-sm font-mono text-foreground">
            {#if $sttStatus?.enabled}
              <span class="inline-flex items-center gap-1.5"><span class="h-2 w-2 rounded-full bg-green-500"></span>{$t('settings.stt_enabled')}</span>
            {:else}
              <span class="inline-flex items-center gap-1.5"><span class="h-2 w-2 rounded-full bg-muted-foreground"></span>{$t('settings.stt_disabled')}</span>
            {/if}
          </span>
        </div>
        <div class="grid grid-cols-1 sm:grid-cols-2 gap-2">
          <span class="text-sm text-muted-foreground">{$t('settings.stt_model_name')}</span>
          <span class="text-sm font-mono text-foreground">
            {#if $sttStatus?.model_loaded}
              <span class="inline-flex items-center gap-1.5"><span class="h-2 w-2 rounded-full bg-green-500"></span>{$sttStatus?.model_name}</span>
            {:else}
              <span class="text-muted-foreground">{$t('settings.stt_model_not_loaded')}</span>
            {/if}
          </span>
        </div>
        <div class="grid grid-cols-1 sm:grid-cols-2 gap-2">
          <span class="text-sm text-muted-foreground">{$t('settings.stt_backend')}</span>
          <span class="text-sm font-mono text-foreground">{$sttStatus?.backend_name ?? "—"}</span>
        </div>
        <div class="grid grid-cols-1 sm:grid-cols-2 gap-2">
          <span class="text-sm text-muted-foreground">{$t('settings.stt_acceleration')}</span>
          <span class="text-sm font-mono text-foreground">
            {#if $sttStatus?.metal_enabled}
              <span class="inline-flex rounded-full bg-primary/10 px-2 py-0.5 text-xs font-medium text-primary">{$t('settings.stt_metal')}</span>
            {:else if $sttStatus?.cuda_enabled}
              <span class="inline-flex rounded-full bg-primary/10 px-2 py-0.5 text-xs font-medium text-primary">{$t('settings.stt_cuda')}</span>
            {:else}
              {$t('settings.stt_none')}
            {/if}
          </span>
        </div>
      </div>
    </div>

    <div class="glass-card glass-border rounded-lg p-4" data-testid="stt-models">
      <h3 class="mb-3 text-sm font-medium uppercase tracking-wider text-muted-foreground">{$t('settings.stt_available_models')}</h3>
      {#if ($sttModelsStore.data ?? []).length === 0}
        <p class="text-sm text-muted-foreground">{$t('settings.stt_no_models')}</p>
      {:else}
        <div class="space-y-2">
          {#each ($sttModelsStore.data ?? []) as model (model.name)}
            <div class="flex items-center justify-between rounded-md border border-border/50 px-3 py-2" data-testid="stt-model-{model.name}">
              <div>
                <span class="text-sm font-mono text-foreground">{model.name}</span>
                {#if model.language}
                  <span class="ml-2 inline-flex rounded-full bg-primary/10 px-2 py-0.5 text-xs font-medium text-primary">{model.language}</span>
                {/if}
              </div>
              <span class="text-xs text-muted-foreground">{$t('settings.stt_model_size', { values: { size: model.size_mb.toFixed(0) } })}</span>
            </div>
          {/each}
        </div>
      {/if}
    </div>

    <div class="rounded-md border border-border/30 bg-muted/20 px-4 py-3 text-xs text-muted-foreground" data-testid="stt-config-hint">
      {$t('settings.stt_config_hint')}
    </div>
  </section>
{/if}
