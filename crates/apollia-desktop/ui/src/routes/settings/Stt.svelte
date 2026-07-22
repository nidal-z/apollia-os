<script lang="ts" context="module">
  import { Input } from "$lib/components/ui/input";
  import { Checkbox } from "$lib/components/ui/checkbox";
  import { Select } from "$lib/components/ui/select";
  export const meta = {
    title: "settings.nav.stt",
    icon: "mic",
    group: "settings.nav.cluster_ai",
    cluster: "ai",
  } as const;
</script>

<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { open as openFilePicker } from "@tauri-apps/plugin-dialog";
  import { homeDir, join as pathJoin } from "@tauri-apps/api/path";
  import { onMount, onDestroy } from "svelte";
  import { t } from "svelte-i18n";
  import { Button } from "$lib/components/ui/button";
  import { ErrorBanner } from "$lib/components/operator";
  import SettingSectionSkeleton from "../../components/settings/SettingSectionSkeleton.svelte";
  import SettingsSection from "../../components/settings/SettingsSection.svelte";
  import HotkeyDisplay from "../../components/settings/HotkeyDisplay.svelte";
  import HotkeyCaptureDialog from "../../components/settings/HotkeyCaptureDialog.svelte";
  import { refreshSttStatus, sttStatus } from "$lib/stores/stt";
  import { sttModelsStore, sttConfigStore, settingsLoaders } from "$lib/stores/settings";
  import { settingsDirtyStore } from "$lib/stores/settingsDirty";
  import { useSettingsForm, registerSettingsForm } from "$lib/settings/useSettingsForm";
  import { addToast } from "$lib/components/ui/toast/store";
  import { formatCombo } from "$lib/keyboard/hotkeyCapture";
  import type { SttConfigView } from "$lib/types";

  let sttConfig = $state<SttConfigView | null>(null);
  // After a save, `update_stt_config` persists and hot-reloads the engine.
  // `sttApplied` shows the change took effect live; `sttNeedsRestart` is the
  // honest fallback shown only when the explicit reload afterwards fails.
  let sttApplied = $state(false);
  let sttNeedsRestart = $state(false);
  let importingSttModel = $state(false);

  const sttState = $derived($settingsDirtyStore.stt ?? {
    dirty: false,
    savingState: "idle" as const,
    lastError: null,
    autoSave: "explicit" as const,
  });
  const sttSaving = $derived(sttState.savingState === "saving");
  const sttSaveError = $derived(sttState.lastError);

  type SttForm = ReturnType<typeof useSettingsForm<SttConfigView>>;
  let form: SttForm | null = null;
  let unregister: (() => void) | null = null;

  $effect(() => {
    const loaded = $sttConfigStore.data;
    if (loaded && !sttConfig) {
      sttConfig = { ...loaded };
      form = useSettingsForm<SttConfigView>({
        route: "stt",
        initial: { ...loaded },
        autoSave: "explicit",
        onSave: async (values) => {
          sttApplied = false;
          sttNeedsRestart = false;
          await invoke("update_stt_config", { config: values });
          // `update_stt_config` already reloads the engine, but it swallows any
          // reload error internally. Reload explicitly (as the Reload button
          // does) so a genuine hot-reload failure surfaces here; only then do
          // we fall back to advising a restart.
          try {
            await invoke("reload_stt");
            await refreshStatus();
            sttApplied = true;
          } catch (err) {
            sttNeedsRestart = true;
            addToast(err instanceof Error ? err.message : String(err), "error");
          }
        },
      });
      unregister = registerSettingsForm<SttConfigView>("stt", form);
    }
  });

  $effect(() => {
    if (sttConfig && form) form.sync($state.snapshot(sttConfig) as SttConfigView);
  });

  async function save() {
    if (!form) return;
    await form.save();
  }

  // Open a native file picker and copy the chosen whisper weights into
  // ~/.apollia/models/, then re-scan so the new file appears in the picker and
  // preselect it.
  async function loadSttModel() {
    if (importingSttModel) return;
    importingSttModel = true;
    try {
      const modelsDir = await pathJoin(await homeDir(), ".apollia", "models");
      const selected = await openFilePicker({
        multiple: false,
        filters: [{ name: $t("settings.stt_model_filter"), extensions: ["bin", "gguf"] }],
        title: $t("settings.stt_load_model_title"),
        defaultPath: modelsDir,
      });
      if (!selected) return;
      const filePath =
        typeof selected === "string" ? selected : (selected as { path: string }).path;
      const dest = await invoke<string>("import_model_file", {
        sourcePath: filePath,
        allowedExtensions: ["bin", "gguf"],
      });
      await settingsLoaders.sttModels(true);
      const name = dest.split(/[\\/]/).pop() ?? "";
      if (name && sttConfig) sttConfig.model_path = `~/.apollia/models/${name}`;
      addToast($t("settings.stt_model_loaded_toast", { values: { name } }), "success");
    } catch (err) {
      addToast(err instanceof Error ? err.message : String(err), "error");
    } finally {
      importingSttModel = false;
    }
  }

  onDestroy(() => {
    unregister?.();
  });

  // ─── Hotkey capture via fullscreen dialog ─────────────────────────────
  let hotkeyDialogOpen = $state(false);

  function openHotkeyCapture() {
    hotkeyDialogOpen = true;
  }

  function onHotkeyConfirm(combo: string) {
    hotkeyDialogOpen = false;
    if (sttConfig) sttConfig.hotkey = combo;
    addToast(
      $t("settings.stt.hotkey.updated_toast", { values: { combo: formatCombo(combo) } }),
      "success",
    );
  }

  function onHotkeyCancel(reason?: "escape" | "timeout" | "close") {
    hotkeyDialogOpen = false;
    if (reason === "timeout") {
      addToast($t("settings.stt.hotkey.timeout_toast"), "info");
    } else {
      addToast($t("settings.stt.hotkey.cancelled_toast"), "info");
    }
  }

  // ─── Status section ───────────────────────────────────────────────────
  let lastStatusRefresh = $state<number | null>(null);
  let liveUpdates = $state(false);
  let now = $state(Date.now());
  let liveTimer: ReturnType<typeof setInterval> | null = null;

  async function refreshStatus() {
    await refreshSttStatus();
    lastStatusRefresh = Date.now();
  }

  // ── Test dictation (G7) ──
  // Reuses the onboarding tour recorder: start records, stop transcribes, and
  // the Rust pipeline broadcasts `stt-transcribed` with the resulting text.
  let testRecording = $state(false);
  let testBusy = $state(false);
  let testResult = $state<string | null>(null);
  let testUnlisten: (() => void) | null = null;

  onMount(() => {
    let cancelled = false;
    void listen<{ text?: string } | string>("stt-transcribed", (event) => {
      if (!testRecording) return;
      const text =
        typeof event.payload === "string" ? event.payload : event.payload?.text ?? "";
      testResult = text;
      testRecording = false;
      testBusy = false;
    }).then((unlisten) => {
      if (cancelled) unlisten();
      else testUnlisten = unlisten;
    });
    return () => {
      cancelled = true;
      testUnlisten?.();
      testUnlisten = null;
    };
  });

  async function toggleTest(): Promise<void> {
    if (testBusy) return;
    testBusy = true;
    try {
      if (testRecording) {
        await invoke("stop_tour_recording");
        // testBusy stays true until the transcription event arrives.
      } else {
        testResult = null;
        await invoke("start_tour_recording");
        testRecording = true;
        testBusy = false;
      }
    } catch (err) {
      testRecording = false;
      testBusy = false;
      addToast(err instanceof Error ? err.message : String(err), "error");
    }
  }

  let reloading = $state(false);

  // Explicit engine reload without re-saving the config. Rebuilds the STT
  // engine from the database and re-arms the global hotkey.
  async function handleReload() {
    if (reloading) return;
    reloading = true;
    try {
      await invoke("reload_stt");
      await refreshStatus();
      addToast($t("settings.stt.reload_toast"), "success");
    } catch (err) {
      addToast(err instanceof Error ? err.message : String(err), "error");
    } finally {
      reloading = false;
    }
  }

  $effect(() => {
    if (liveUpdates) {
      liveTimer = setInterval(() => { void refreshStatus(); }, 5000);
    } else if (liveTimer) {
      clearInterval(liveTimer);
      liveTimer = null;
    }
    return () => {
      if (liveTimer) { clearInterval(liveTimer); liveTimer = null; }
    };
  });

  // Tick a relative-time clock once per second so `status_updated` refreshes.
  $effect(() => {
    const id = setInterval(() => { now = Date.now(); }, 1000);
    return () => clearInterval(id);
  });

  const relativeTime = $derived.by(() => {
    if (lastStatusRefresh === null) return $t("settings.stt.status_never");
    const deltaSec = Math.round((lastStatusRefresh - now) / 1000);
    try {
      const rtf = new Intl.RelativeTimeFormat(undefined, { numeric: "auto" });
      const abs = Math.abs(deltaSec);
      if (abs < 60) return $t("settings.stt.status_updated", { values: { relative: rtf.format(deltaSec, "second") } });
      if (abs < 3600) return $t("settings.stt.status_updated", { values: { relative: rtf.format(Math.round(deltaSec / 60), "minute") } });
      return $t("settings.stt.status_updated", { values: { relative: rtf.format(Math.round(deltaSec / 3600), "hour") } });
    } catch {
      return $t("settings.stt.status_updated", { values: { relative: `${Math.abs(deltaSec)}s` } });
    }
  });

  onMount(() => {
    void settingsLoaders.sttConfig();
    void settingsLoaders.sttModels();
    void refreshStatus();
  });
</script>

{#if $sttConfigStore.loading && !sttConfig}
  <SettingSectionSkeleton rows={2} />
{:else if $sttConfigStore.error}
  <ErrorBanner message={`${$t('settings.stt_error')}: ${$sttConfigStore.error}`} />
{:else if sttConfig}
  <section class="space-y-5" data-testid="stt-section">
    <p class="text-sm text-muted-foreground">{$t('settings.stt_subtitle')}</p>

    <!-- ─── Section 1 : Essential ───────────────────────────────────────── -->
    <div data-testid="stt-config-form" class="relative">
      <SettingsSection
        title={$t('settings.stt.section.essential_title')}
        description={$t('settings.stt.section.essential_description')}
        data-testid="stt-section-essential"
      >
        {#snippet actions()}
          {#if sttState.dirty}
            <div class="sticky top-2 z-10" data-testid="stt-sticky-save">
              <Button
                size="sm"
                onclick={save}
                disabled={sttSaving || !sttState.dirty}
                loading={sttSaving}
                data-testid="stt-save-btn"
              >
                {sttSaving ? $t('settings.stt_saving') : $t('settings.stt_save')}
              </Button>
            </div>
          {/if}
        {/snippet}

        <label class="flex cursor-pointer items-center justify-between" data-testid="stt-enable-toggle">
          <span class="text-sm text-foreground">{$t('settings.stt_enable_engine')}</span>
          <Button variant="ghost" size="sm"
            type="button"
            role="switch"
            aria-checked={sttConfig.enabled}
            aria-label={$t('settings.stt_enable_engine')}
            onclick={() => { if (sttConfig) sttConfig.enabled = !sttConfig.enabled; }}
            class="relative inline-flex h-6 w-11 items-center rounded-full transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring {sttConfig.enabled ? 'bg-primary' : 'bg-muted'}"
          >
            <span class="inline-block h-4 w-4 rounded-full bg-white shadow-sm transition-transform {sttConfig.enabled ? 'translate-x-6' : 'translate-x-1'}"></span>
          </Button>
        </label>

        <div class="space-y-1.5">
          <label class="text-sm text-muted-foreground" for="stt-model-select">{$t('settings.stt_select_model')}</label>
          {#if ($sttModelsStore.data ?? []).length > 0}
            <Select
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
            </Select>
          {:else}
            <p class="rounded-md border border-border/50 px-3 py-2 text-sm text-muted-foreground">
              {$t('settings.stt_no_models')}
            </p>
          {/if}
          <div class="flex items-center gap-2 pt-1">
            <Button
              size="sm"
              onclick={loadSttModel}
              disabled={importingSttModel}
              loading={importingSttModel}
              data-testid="stt-load-model-btn"
            >
              {$t('settings.stt_load_model')}
            </Button>
            <span class="text-xs text-muted-foreground">{$t('settings.stt_load_model_hint')}</span>
          </div>
        </div>

        <div class="space-y-1.5">
          <label class="text-sm text-muted-foreground" for="stt-language">{$t('settings.stt_language')}</label>
          <Input
            id="stt-language"
            type="text"
            placeholder={$t('settings.stt_language_auto')}
            bind:value={sttConfig.language}
            class="flex h-9 w-full rounded-md border border-border bg-background px-3 py-1.5 text-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/40"
            data-testid="stt-language-input"
           />
        </div>

        <div class="space-y-1.5">
          <label class="text-sm text-muted-foreground" for="stt-trigger">{$t('settings.stt_trigger_mode')}</label>
          <Select
            id="stt-trigger"
            data-testid="stt-trigger"
            bind:value={sttConfig.trigger_mode}
            class="flex h-9 w-full appearance-none rounded-md border border-border bg-background px-3 py-1.5 text-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/40"
          >
            <option value="toggle">{$t('settings.stt_trigger_toggle')}</option>
            <option value="push-to-talk">{$t('settings.stt_trigger_push')}</option>
          </Select>
        </div>

        {#snippet footer()}
          {#if sttState.savingState === "saved"}
            <span class="text-sm text-emerald-600 dark:text-emerald-400" data-testid="stt-save-status-saved">
              {$t('settings.save_status_saved')}
            </span>
          {:else if sttSaveError}
            <span class="text-sm text-destructive" data-testid="stt-save-status-error">
              {$t('settings.save_status_retry')}: {sttSaveError}
            </span>
          {/if}
        {/snippet}
      </SettingsSection>
    </div>

    <!-- ─── Section 2 : Advanced Triggers ───────────────────────────────── -->
    <SettingsSection
      title={$t('settings.stt.section.advanced_title')}
      description={$t('settings.stt.section.advanced_description')}
      data-testid="stt-section-advanced"
    >
      <div class="space-y-1.5">
        <span class="text-sm text-muted-foreground">{$t('settings.stt.hotkey.label')}</span>
        <div class="flex items-center justify-between gap-3 rounded-md border border-border/60 bg-background px-3 py-2">
          <HotkeyDisplay
            hotkey={sttConfig.hotkey}
            placeholder={$t('settings.stt.hotkey.preview_placeholder')}
            data-testid="stt-hotkey-preview"
          />
          <Button
            variant="outline"
            size="sm"
            onclick={openHotkeyCapture}
            data-testid="stt-hotkey-change-btn"
          >
            {$t('settings.stt.hotkey.change')}
          </Button>
        </div>
      </div>

      <div class="space-y-1.5">
        <label class="text-sm text-muted-foreground" for="stt-silence">{$t('settings.stt_silence_threshold')}</label>
        <div class="relative">
          <Input
            id="stt-silence"
            data-testid="stt-silence"
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
    </SettingsSection>

    <!-- ─── Section 3 : Performance ─────────────────────────────────────── -->
    <SettingsSection
      title={$t('settings.stt.section.performance_title')}
      description={$t('settings.stt.section.performance_description')}
      data-testid="stt-section-performance"
    >
      <div class="grid grid-cols-1 sm:grid-cols-2 gap-4">
        <div class="space-y-1.5">
          <label class="text-sm text-muted-foreground" for="stt-max-rec">{$t('settings.stt_max_recording')}</label>
          <div class="relative">
            <Input
              id="stt-max-rec"
              data-testid="stt-max-rec"
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
          <label class="text-sm text-muted-foreground" for="stt-clipboard">{$t('settings.stt_clipboard_mode')}</label>
          <Select
            id="stt-clipboard"
            data-testid="stt-clipboard"
            bind:value={sttConfig.clipboard_mode}
            class="flex h-9 w-full appearance-none rounded-md border border-border bg-background px-3 py-1.5 text-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/40"
          >
            <option value="paste">{$t('settings.stt_clipboard_paste')}</option>
            <option value="clipboard">{$t('settings.stt_clipboard_clipboard')}</option>
          </Select>
        </div>
      </div>

      <label class="flex cursor-pointer items-center justify-between">
        <span class="text-sm text-muted-foreground">{$t('settings.stt_clipboard_restore')}</span>
        <Button variant="ghost" size="sm"
          type="button"
          role="switch"
          data-testid="stt-clipboard-restore"
          aria-checked={sttConfig.clipboard_restore}
          aria-label={$t('settings.stt_clipboard_restore')}
          onclick={() => { if (sttConfig) sttConfig.clipboard_restore = !sttConfig.clipboard_restore; }}
          class="relative inline-flex h-6 w-11 items-center rounded-full transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring {sttConfig.clipboard_restore ? 'bg-primary' : 'bg-muted'}"
        >
          <span class="inline-block h-4 w-4 rounded-full bg-white shadow-sm transition-transform {sttConfig.clipboard_restore ? 'translate-x-6' : 'translate-x-1'}"></span>
        </Button>
      </label>

      <div class="grid grid-cols-1 sm:grid-cols-2 gap-2 border-t border-border/40 pt-3">
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
    </SettingsSection>

    <!-- ─── Section 4 : Status ──────────────────────────────────────────── -->
    <SettingsSection
      title={$t('settings.stt.section.status_title')}
      description={$t('settings.stt.section.status_description')}
      data-testid="stt-section-status"
    >
      {#snippet actions()}
        <div class="flex items-center gap-3">
          <label class="flex cursor-pointer items-center gap-2 text-xs text-muted-foreground">
            <Checkbox bind:checked={liveUpdates} data-testid="stt-status-live-toggle" />
            {$t('settings.stt.status_live_updates')}
          </label>
          <Button
            variant="outline"
            size="sm"
            onclick={handleReload}
            disabled={reloading}
            data-testid="stt-reload-btn"
          >
            {$t('settings.stt.reload_btn')}
          </Button>
          <Button
            variant="outline"
            size="sm"
            onclick={refreshStatus}
            data-testid="stt-status-refresh-btn"
          >
            {$t('settings.stt.status_refresh')}
          </Button>
        </div>
      {/snippet}

      <!-- Test dictation (G7): record a phrase and show the transcription. -->
      <div class="mb-3 flex flex-col gap-2 rounded-md border border-border/40 bg-muted/20 p-3">
        <div class="flex items-center justify-between gap-2">
          <span class="text-sm text-muted-foreground">{$t('settings.stt.test.label')}</span>
          <Button
            variant={testRecording ? "destructive" : "outline"}
            size="sm"
            onclick={toggleTest}
            disabled={testBusy && !testRecording}
            data-testid={testRecording ? "stt-test-stop" : "stt-test-start"}
          >
            {#if testRecording}
              {$t('settings.stt.test.stop')}
            {:else if testBusy}
              {$t('settings.stt.test.transcribing')}
            {:else}
              {$t('settings.stt.test.start')}
            {/if}
          </Button>
        </div>
        {#if testResult !== null}
          <p class="rounded bg-background px-2 py-1 text-sm text-foreground" data-testid="stt-test-result">
            {testResult || $t('settings.stt.test.empty')}
          </p>
        {/if}
      </div>

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
        <span class="text-sm font-mono text-foreground">{$sttStatus?.backend_name ?? "-"}</span>
      </div>

      {#snippet footer()}
        <span class="text-xs text-muted-foreground" data-testid="stt-status-timestamp">{relativeTime}</span>
      {/snippet}
    </SettingsSection>

    {#if sttApplied}
      <div class="flex items-start gap-3 rounded-md border border-emerald-500/30 bg-emerald-500/10 px-4 py-3 text-sm text-emerald-700 dark:text-emerald-400" data-testid="stt-applied-notice" role="status">
        {$t('settings.stt_applied_notice')}
      </div>
    {:else if sttNeedsRestart}
      <div class="flex items-start gap-3 rounded-md border border-amber-500/30 bg-amber-500/10 px-4 py-3 text-sm text-amber-700 dark:text-amber-400" data-testid="stt-restart-notice" role="alert">
        {$t('settings.stt_restart_notice')}
      </div>
    {/if}

    <div class="rounded-md border border-border/30 bg-muted/20 px-4 py-3 text-xs text-muted-foreground" data-testid="stt-config-hint">
      {$t('settings.stt_config_hint')}
    </div>
  </section>

  <HotkeyCaptureDialog
    open={hotkeyDialogOpen}
    onconfirm={onHotkeyConfirm}
    oncancel={onHotkeyCancel}
  />
{/if}
