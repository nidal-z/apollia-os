<!--
  Onboarding step 3, the voice half.

  Scans for Whisper weights, lets the operator pick one and turn dictation on,
  then set the push-to-talk shortcut, choose a microphone and test the pipeline
  without leaving the step. The step shell holds the navigation; the selection
  it needs on continue is reported through `onchange`.
-->
<script lang="ts">
  import { onDestroy } from "svelte";
  import { get } from "svelte/store";
  import { t } from "svelte-i18n";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { open as openFilePicker } from "@tauri-apps/plugin-dialog";
  import {
    cancelModelDownload,
    getSttConfig,
    importModelFile,
    listAudioInputDevices,
    reloadStt,
    scanForWhisperModels,
    setupWhisperModel,
    startModelDownload,
    startTourRecording,
    stopTourRecording,
    updateSttConfig,
    type DownloadProgress,
    type SystemInfo,
    type WhisperModelInfo,
  } from "$lib/ipc/models";
  import {
    Mic,
    Check,
    ChevronRight,
    AlertCircle,
    Download,
    X,
    RefreshCw,
    Upload,
  } from "lucide-svelte";
  import { ProgressBar } from "$lib/components/ui/progress";
  import { Button } from "$lib/components/ui/button";
  import { Checkbox } from "$lib/components/ui/checkbox";
  import HotkeyCaptureDialog from "../settings/HotkeyCaptureDialog.svelte";
  import { formatCombo } from "$lib/keyboard/hotkeyCapture";
  import { ensureMicPermission } from "$lib/stt/micPermission";
  import {
    DICTATION_FAILED_EVENT,
    failureMessageKey,
    readFailureReason,
  } from "$lib/stt/dictationFailure";
  import { reconcileWhisperScan, sttSectionView } from "./aiSetupRules";
  import {
    CURATED_STT_MODELS,
    modelsFitting,
    largestFitting,
    type CuratedSttModel,
  } from "./onboardingCatalogs";
  import { dlBytes, dlPct, dlSpeed, pickModelsDir } from "./onboardingFormat";
  import "./onboarding-stt-controls.css";

  interface Props {
    /** Drives which curated models are offered; `null` until the probe lands. */
    sysInfo: SystemInfo | null;
    /** The base app locale, so dictation transcribes in the operator's tongue. */
    locale: () => string | undefined;
    /** Reports the voice choice the step must persist before it advances. */
    onchange: (choice: { enabled: boolean; model: WhisperModelInfo | null }) => void;
  }

  const { sysInfo, locale, onchange }: Props = $props();

  let whisperModels = $state<WhisperModelInfo[]>([]);
  let selectedWhisper = $state<WhisperModelInfo | null>(null);
  let sttEnabled = $state(false);
  let importingStt = $state(false);

  let sttDownloadId = $state<string | null>(null);
  let sttDownloadProgress = $state<DownloadProgress | null>(null);
  let sttDownloadingModel = $state<CuratedSttModel | null>(null);
  let sttDownloadError = $state<string | null>(null);

  // ── STT hotkey + live test ────────────────────────────────────────────
  // Reads the current config (the hotkey persisted in system.db), lets the
  // operator set it from the keyboard, and tests the pipeline live without
  // leaving onboarding. The transcription arrives on the Tauri event
  // `stt-transcribed`.
  let sttHotkey = $state<string>("");
  let sttHotkeyDirty = $state(false);
  let sttHotkeySaving = $state(false);
  let sttHotkeyCapturing = $state(false);
  let sttHotkeyError = $state<string | null>(null);
  let sttTesting = $state(false);
  let sttTestRecording = $state(false);
  let sttTestTranscript = $state<string | null>(null);
  let sttTestError = $state<string | null>(null);
  let sttTestUnlisten: (() => void) | null = null;

  // ── Audio input device selection ─────────────────────────────────────
  // Empty string = system default microphone. An empty device list means no
  // microphone is connected, which we surface so the user is not left with a
  // silent test.
  let sttInputDevices = $state<string[]>([]);
  let sttInputDevicesLoaded = $state(false);
  let sttInputDevice = $state<string>("");
  const noMicrophone = $derived(
    sttInputDevicesLoaded && sttInputDevices.length === 0,
  );

  const availableSttModels = $derived(modelsFitting(CURATED_STT_MODELS, sysInfo));
  const recommendedStt = $derived.by((): CuratedSttModel | null => {
    if (!sysInfo) return CURATED_STT_MODELS[2] ?? null;
    const turbo = CURATED_STT_MODELS.find((m) => m.filename.includes("turbo"));
    if (turbo && sysInfo.total_ram_gb >= turbo.ram_required) return turbo;
    return largestFitting(CURATED_STT_MODELS, sysInfo, 2);
  });
  const sttView = $derived(sttSectionView(whisperModels.length, sttEnabled));

  $effect(() => {
    void loadData();
  });

  $effect(() => {
    let unlisten: (() => void) | undefined;
    listen<DownloadProgress>("model-download-progress", (event) => {
      const p = event.payload;
      if (p.id !== sttDownloadId) return;
      sttDownloadProgress = p;
      if (p.status === "completed") {
        sttDownloadId = null;
        sttDownloadingModel = null;
        void rescanStt();
      } else if (p.status === "cancelled" || p.status === "failed") {
        sttDownloadId = null;
        sttDownloadingModel = null;
        sttDownloadProgress = null;
        if (p.status === "failed")
          sttDownloadError = get(t)("onboarding.ai_setup.download_failed");
      }
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      unlisten?.();
    };
  });

  // The shell persists the model before it advances, so it is told every time
  // the pair changes rather than asked for it on the way out.
  $effect(() => {
    onchange({ enabled: sttEnabled, model: selectedWhisper });
  });

  onDestroy(() => {
    if (sttTestUnlisten !== null) {
      sttTestUnlisten();
      sttTestUnlisten = null;
    }
  });

  /**
   * Apply a fresh voice scan to the section.
   *
   * Every path that scans for voice models goes through here, so a re-scan
   * behaves the same whether it was triggered by the header button, by a
   * finished download, by an import from disk, or by the rescan link of the
   * language-engine section.
   */
  function applyWhisperScan(scanned: WhisperModelInfo[]): void {
    whisperModels = scanned;
    const choice = reconcileWhisperScan(
      scanned,
      selectedWhisper?.path ?? null,
      sttEnabled,
    );
    selectedWhisper =
      scanned.find((model) => model.path === choice.selectedPath) ?? null;
    sttEnabled = choice.dictationEnabled;
  }

  async function loadData(): Promise<void> {
    try {
      const [whisper, sttCfg] = await Promise.all([
        scanForWhisperModels(),
        (
          getSttConfig() as Promise<{
            hotkey?: string;
            input_device?: string | null;
          }>
        ).catch(() => ({}) as { hotkey?: string; input_device?: string | null }),
      ]);
      applyWhisperScan(whisper);
      sttHotkey = sttCfg?.hotkey ?? "ctrl+shift+space";
      sttInputDevice = sttCfg?.input_device ?? "";
      void loadInputDevices();
    } catch {
      /* leave empty */
    }
  }

  async function rescanStt(): Promise<void> {
    try {
      applyWhisperScan(await scanForWhisperModels());
    } catch {
      /* leave empty */
    }
  }

  async function loadInputDevices(): Promise<void> {
    try {
      sttInputDevices = await listAudioInputDevices();
    } catch {
      sttInputDevices = [];
    } finally {
      sttInputDevicesLoaded = true;
    }
  }

  async function onSelectInputDevice(value: string): Promise<void> {
    sttInputDevice = value;
    try {
      // Patch only input_device onto the persisted config so the other fields
      // (enabled, model_path, hotkey, ...) are preserved.
      const current = await getSttConfig();
      await updateSttConfig({ ...current, input_device: value || null });
    } catch (err) {
      console.error("update_stt_config (input_device) failed", err);
    }
  }

  // Hotkey capture is delegated to the shared `HotkeyCaptureDialog`
  // (`crates/apollia-desktop/ui/src/components/settings/HotkeyCaptureDialog.svelte`)
  // which uses `event.code` rather than `event.key` - this fixes macOS
  // Option+key (which would otherwise yield Greek/accented characters
  // because Option produces Unicode dead-keys) and properly captures Space.
  function startHotkeyCapture(): void {
    sttHotkeyError = null;
    sttHotkeyCapturing = true;
  }

  function onHotkeyConfirm(combo: string): void {
    sttHotkey = combo;
    sttHotkeyDirty = true;
    sttHotkeyCapturing = false;
  }

  function onHotkeyCancel(): void {
    sttHotkeyCapturing = false;
  }

  async function saveSttHotkey(): Promise<void> {
    if (!sttHotkey.trim()) return;
    sttHotkeySaving = true;
    try {
      // update_stt_config requires the full config; fetch the persisted one and
      // patch only the hotkey so we do not drop the other fields (enabled,
      // model_path, ...), which previously failed with "missing field enabled".
      const current = await getSttConfig();
      await updateSttConfig({ ...current, hotkey: sttHotkey.trim() });
      sttHotkeyDirty = false;
    } catch (err) {
      // Surface inline; the existing error pattern in this view is the
      // simple inline-error span used by other actions.
      console.error("update_stt_config failed", err);
    } finally {
      sttHotkeySaving = false;
    }
  }

  function attachSttTestListener(): void {
    if (sttTestUnlisten !== null) return;
    const unlisteners: UnlistenFn[] = [];
    void listen<{ text?: string } | string>("stt-transcribed", (event) => {
      const text =
        typeof event.payload === "string"
          ? event.payload
          : event.payload?.text ?? "";
      sttTestTranscript = text || get(t)("onboarding_stt.test_empty");
      sttTestError = null;
      sttTesting = false;
      sttTestRecording = false;
    }).then((unlisten) => {
      unlisteners.push(unlisten);
    });
    // A test that captures silence never produces a transcription. Without
    // this the button stays on "stop" and the step cannot be completed.
    void listen(DICTATION_FAILED_EVENT, (event) => {
      sttTestTranscript = null;
      sttTestError = get(t)(failureMessageKey(readFailureReason(event.payload)));
      sttTesting = false;
      sttTestRecording = false;
    }).then((unlisten) => {
      unlisteners.push(unlisten);
    });
    sttTestUnlisten = () => {
      for (const unlisten of unlisteners) unlisten();
      unlisteners.length = 0;
    };
  }

  async function startSttTest(): Promise<void> {
    sttTestTranscript = null;
    sttTestError = null;
    sttTesting = true;
    sttTestRecording = true;
    attachSttTestListener();
    try {
      // Raise the microphone permission prompt before capturing. The cpal
      // stream shares the app-level TCC grant, but nothing native triggers the
      // prompt, so without this the capture reads silence (flat waveform, empty
      // transcription) on a fresh install.
      await ensureMicPermission();
      // Selecting a whisper model in onboarding does not persist it, so the
      // engine (and the push-to-talk flow behind start_tour_recording) may not
      // exist yet - hence the old "STT engine not available" error. Persist the
      // selected model and hot-reload the engine before testing; a plain reload
      // covers the case where a model is already configured in system.db.
      if (selectedWhisper) {
        await setupWhisperModel(selectedWhisper.path, locale());
      } else {
        await reloadStt();
      }
      await startTourRecording();
    } catch {
      // Honest retry: rebuild the engine once, then surface the failure only
      // if the reload genuinely could not bring the engine online.
      try {
        await reloadStt();
        await startTourRecording();
      } catch (err2) {
        sttTestRecording = false;
        sttTesting = false;
        sttTestTranscript = null;
        sttTestError = get(t)("onboarding_stt.test_error", {
          values: { error: String(err2) },
        });
      }
    }
  }

  async function stopSttTest(): Promise<void> {
    if (!sttTestRecording) return;
    try {
      await stopTourRecording();
      // sttTesting stays true until the transcribed event fires (or fails).
    } catch (err) {
      sttTestRecording = false;
      sttTesting = false;
      sttTestTranscript = null;
      sttTestError = get(t)("onboarding_stt.test_error", {
        values: { error: String(err) },
      });
    }
  }

  async function loadSttModelFile(): Promise<void> {
    if (importingStt) return;
    importingStt = true;
    sttDownloadError = null;
    try {
      const selected = await openFilePicker({
        multiple: false,
        filters: [{ name: "Whisper", extensions: ["bin", "gguf"] }],
        title: get(t)("onboarding.ai_setup.load_model_title"),
        defaultPath: await pickModelsDir(),
      });
      if (!selected) return;
      const filePath =
        typeof selected === "string" ? selected : (selected as { path: string }).path;
      await importModelFile(filePath, ["bin", "gguf"]);
      await rescanStt();
    } catch (err: unknown) {
      sttDownloadError = err instanceof Error ? err.message : String(err);
    } finally {
      importingStt = false;
    }
  }

  async function downloadSttModel(model: CuratedSttModel): Promise<void> {
    if (sttDownloadId) return;
    sttDownloadError = null;
    sttDownloadProgress = null;
    sttDownloadingModel = model;
    try {
      sttDownloadId = await startModelDownload({
        url: model.url,
        filename: model.filename,
      });
    } catch (err: unknown) {
      sttDownloadError = err instanceof Error ? err.message : String(err);
      sttDownloadingModel = null;
    }
  }

  async function cancelSttDownload(): Promise<void> {
    if (!sttDownloadId) return;
    try {
      await cancelModelDownload(sttDownloadId);
    } catch {
      /* ignore */
    }
  }
</script>

<section class="setup-section" data-testid="stt-section">
  <div class="section-header">
    <Mic size={14} strokeWidth={2} class="text-secondary" />
    <span class="section-title">{$t("onboarding.ai_setup.stt_section_title")}</span>
    <Button variant="ghost" size="sm" class="btn-rescan" onclick={rescanStt} aria-label={$t("onboarding.ai_setup.rescan_whisper")} title={$t("onboarding.ai_setup.rescan_short")}>
      <RefreshCw size={11} strokeWidth={2} />
    </Button>
    <label class="toggle-label">
      <Checkbox class="toggle-input" bind:checked={sttEnabled} disabled={whisperModels.length === 0} data-testid="stt-toggle" />
      <span class="toggle-track" class:on={sttEnabled}></span>
    </label>
  </div>

  {#if noMicrophone}
    <p class="stt-no-mic" role="alert" data-testid="stt-no-microphone">
      <AlertCircle size={12} />{$t("onboarding_stt.no_microphone")}
    </p>
  {/if}

  {#if sttView.showEmptyHint}
    <p class="empty-hint" data-testid="stt-empty-hint">
      {$t("onboarding.ai_setup.stt_empty_hint")}
    </p>
  {/if}

  {#if sttView.showDetectedList}
    <ul class="model-list" data-testid="whisper-model-list">
      {#each whisperModels as model (model.path)}
        <li>
          <button
            class="model-row"
            class:is-selected={selectedWhisper?.path === model.path}
            onclick={() => { selectedWhisper = model; }}
            data-testid="whisper-model-row"
          >
            <div class="model-icon model-icon-stt"><Mic size={12} strokeWidth={1.75} /></div>
            <div class="model-info">
              <span class="model-name">{model.filename}</span>
              <span class="model-meta capitalize">{model.model_size}</span>
            </div>
            {#if model.recommended}
              <span class="badge-recommended badge-recommended-stt">{$t("onboarding.ai_setup.recommended")}</span>
            {/if}
            {#if selectedWhisper?.path === model.path}
              <Check size={13} strokeWidth={2.5} class="text-secondary" />
            {:else}
              <ChevronRight size={13} class="text-muted-foreground/50" />
            {/if}
          </button>
        </li>
      {/each}
    </ul>
  {/if}

  {#if sttView.showAddMean}
    <div class="load-model-row">
      <Button
        variant="default"
        size="sm"
        onclick={loadSttModelFile}
        disabled={importingStt}
        loading={importingStt}
        data-testid="stt-load-model-btn"
      >
        <Upload size={12} strokeWidth={2} />
        {$t("onboarding.ai_setup.load_model")}
      </Button>
    </div>
  {/if}

  {#if sttDownloadId}
    <div class="download-block" data-testid="stt-download-progress">
      <div class="dl-header">
        <span class="dl-filename">{sttDownloadingModel?.filename ?? "…"}</span>
        <Button variant="ghost" size="sm" class="btn-cancel-dl" onclick={cancelSttDownload} aria-label={$t("onboarding.ai_setup.cancel_download")}>
          <X size={12} strokeWidth={2} />
        </Button>
      </div>
      <ProgressBar
        value={sttDownloadProgress ? dlPct(sttDownloadProgress) : undefined}
        size="sm"
        variant="primary"
      />
      {#if sttDownloadProgress}
        <div class="dl-meta">
          <span>{dlBytes(sttDownloadProgress)}</span>
          <span>{dlSpeed(sttDownloadProgress.speed_bps)}</span>
        </div>
      {/if}
    </div>
  {:else if sttView.showCuratedList}
    <div class="curated-divider"><span>{$t("onboarding.ai_setup.whisper_models")}</span></div>
    <ul class="model-list" data-testid="curated-stt-list">
      {#each availableSttModels as model (model.filename)}
        <li>
          <button type="button" class="model-row" onclick={() => downloadSttModel(model)} data-testid="curated-stt-row">
            <div class="model-icon model-icon-stt"><Download size={12} strokeWidth={1.75} /></div>
            <div class="model-info">
              <span class="model-name">{model.name}</span>
              <span class="model-meta">
                {model.size_label} · {$t(model.quality_key)} ·
                <span class="model-lang">{$t(model.lang_key)}</span>
              </span>
            </div>
            {#if model.filename === recommendedStt?.filename}
              <span class="badge-recommended badge-recommended-stt">{$t("onboarding.ai_setup.recommended")}</span>
            {/if}
            <ChevronRight size={13} class="text-muted-foreground/50" />
          </button>
        </li>
      {/each}
    </ul>
  {/if}

  {#if sttDownloadError}
    <p class="inline-error" role="alert" data-testid="stt-download-error">
      <AlertCircle size={12} />{sttDownloadError}
    </p>
  {/if}

  {#if sttView.showHotkeyBlock}
    <!-- ── Raccourci (capture clavier) + Test live ────────────────── -->
    <div class="stt-hotkey-block" data-testid="stt-hotkey-block">
      <div class="hotkey-row">
        <span class="hotkey-label">{$t("onboarding_stt.mic_label")}</span>
        <select
          class="mic-select"
          value={sttInputDevice}
          onchange={(e) => onSelectInputDevice((e.currentTarget as HTMLSelectElement).value)}
          disabled={noMicrophone}
          data-testid="stt-input-device"
        >
          <option value="">{$t("onboarding_stt.mic_default")}</option>
          {#each sttInputDevices as device (device)}
            <option value={device}>{device}</option>
          {/each}
        </select>
      </div>
      <div class="hotkey-row">
        <span class="hotkey-label">{$t("onboarding_stt.hotkey_label")}</span>
        <Button variant="ghost" size="sm"
          type="button"
          class="hotkey-capture"
          onclick={startHotkeyCapture}
          data-testid="stt-hotkey-capture"
        >
          {#if sttHotkey}
            <code class="hotkey-display">{formatCombo(sttHotkey)}</code>
          {:else}
            <span class="hotkey-placeholder">
              {$t("onboarding_stt.hotkey_capture_idle")}
            </span>
          {/if}
        </Button>
        <Button variant="ghost" size="sm"
          type="button"
          class="hotkey-save"
          onclick={saveSttHotkey}
          disabled={!sttHotkeyDirty || sttHotkeySaving}
          data-testid="stt-hotkey-save"
        >
          {sttHotkeySaving
            ? $t("onboarding_stt.hotkey_saving")
            : $t("onboarding_stt.hotkey_save")}
        </Button>
      </div>
      <p class="hotkey-hint">{$t("onboarding_stt.hotkey_hint")}</p>
      {#if sttHotkeyError}
        <p class="inline-error">{sttHotkeyError}</p>
      {/if}

      <div class="stt-test-row">
        {#if !sttTestRecording}
          <Button variant="ghost" size="sm"
            type="button"
            class="stt-test-btn stt-test-start"
            onclick={startSttTest}
            disabled={sttTesting}
            data-testid="stt-test-start"
          >
            <Mic size={12} /> {$t("onboarding_stt.test_start")}
          </Button>
        {:else}
          <Button variant="ghost" size="sm"
            type="button"
            class="stt-test-btn stt-test-stop"
            onclick={stopSttTest}
            data-testid="stt-test-stop"
          >
            <Mic size={12} /> {$t("onboarding_stt.test_stop")}
          </Button>
        {/if}
        {#if sttTestError !== null}
          <span class="stt-test-error" role="alert" data-testid="stt-test-error">
            <AlertCircle size={12} /> {sttTestError}
          </span>
        {:else if sttTestTranscript !== null}
          <span class="stt-test-transcript" data-testid="stt-test-transcript">
            <Check size={12} /> «&nbsp;{sttTestTranscript}&nbsp;»
          </span>
        {/if}
      </div>
    </div>
  {/if}
</section>

<HotkeyCaptureDialog
  open={sttHotkeyCapturing}
  onconfirm={onHotkeyConfirm}
  oncancel={onHotkeyCancel}
/>
