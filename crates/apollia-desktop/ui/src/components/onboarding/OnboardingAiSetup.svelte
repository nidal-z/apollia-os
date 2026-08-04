<script lang="ts">
  /**
   * Onboarding step 3 - AI Setup.
   *
   * Configures the local LLM (llama.cpp + GGUF) and the optional STT engine
   * (whisper.cpp). Scans `~/.apollia/models/` and `~/Downloads/` for
   * existing weights, exposes a curated list of recommended models with
   * one-click download, and surfaces a HuggingFace search for power users.
   *
   * Designed to render inline inside the {@link OnboardingModal} overlay
   * (no own backdrop or fixed positioning).
   */
  import { onDestroy } from "svelte";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { open as openFilePicker } from "@tauri-apps/plugin-dialog";
  import { homeDir, join as pathJoin } from "@tauri-apps/api/path";
  import { get } from "svelte/store";
  import { t, locale } from "svelte-i18n";
  import {
    cancelModelDownload,
    getAiSetupInfo,
    getHfModel,
    getSttConfig,
    importModelFile,
    listAudioInputDevices,
    reloadLlm,
    reloadStt,
    scanForGgufModels,
    scanForWhisperModels,
    searchHfModels,
    setupLocalLlm,
    setupWhisperModel,
    startModelDownload,
    startTourRecording,
    stopTourRecording,
    updateSttConfig,
    type DownloadProgress,
    type GgufModelInfo,
    type HfFile,
    type HfModelCard,
    type SystemInfo,
    type WhisperModelInfo,
  } from "$lib/ipc/models";

  /** Base app locale (e.g. "fr", "en") to seed the STT transcription language,
   * so dictation transcribes in the user's language instead of English. */
  function currentLocale(): string | undefined {
    const l = get(locale);
    return l ? l.split("-")[0] : undefined;
  }
  import HotkeyCaptureDialog from "../settings/HotkeyCaptureDialog.svelte";
  import { formatCombo } from "$lib/keyboard/hotkeyCapture";
  import { ensureMicPermission } from "$lib/stt/micPermission";
  import {
    DICTATION_FAILED_EVENT,
    failureMessageKey,
    readFailureReason,
  } from "$lib/stt/dictationFailure";
  import {
    Cpu,
    HardDrive,
    Mic,
    Check,
    ChevronRight,
    AlertCircle,
    MemoryStick,
    MonitorCog,
    Download,
    X,
    Search,
    ArrowLeft,
    RefreshCw,
    Cloud,
    Upload,
  } from "lucide-svelte";
  import { Spinner, ProgressBar } from "$lib/components/ui/progress";
  import { Button } from "$lib/components/ui/button";
  import { llmBackends } from "$lib/stores/sse";
  import { navigateTo } from "$lib/stores/navigation";
  import { Input } from "$lib/components/ui/input";
  import { Checkbox } from "$lib/components/ui/checkbox";

  interface Props {
    onnext: () => void;
    onback: () => void;
    onskip: () => void;
    /** Called when the user wants to add a cloud backend; modal should close. */
    onopencloud: () => void;
  }

  const { onnext, onback, onskip, onopencloud }: Props = $props();

  // ─── Types ────────────────────────────────────────────────────────────────

  interface CuratedLlmModel {
    name: string;
    filename: string;
    url: string;
    size_label: string;
    ram_required: number;
  }

  interface CuratedSttModel {
    name: string;
    filename: string;
    url: string;
    repo: string;
    size_label: string;
    ram_required: number;
    quality_key: string;
    lang_key: string;
  }

  // ─── Curated catalogs ─────────────────────────────────────────────────────
  // Qwen3 (April 2025): native tool calling via llama.cpp jinja templates.
  // Whisper turbo-q5 is the sweet spot: pruned from large-v3, 6× faster.

  const CURATED_LLM_MODELS: CuratedLlmModel[] = [
    {
      name: "Qwen3 4B",
      filename: "Qwen3-4B-Q4_K_M.gguf",
      url: "https://huggingface.co/Qwen/Qwen3-4B-GGUF/resolve/main/Qwen3-4B-Q4_K_M.gguf",
      size_label: "2.5 GB",
      ram_required: 4,
    },
    {
      name: "Qwen3 8B",
      filename: "Qwen3-8B-Q4_K_M.gguf",
      url: "https://huggingface.co/Qwen/Qwen3-8B-GGUF/resolve/main/Qwen3-8B-Q4_K_M.gguf",
      size_label: "4.7 GB",
      ram_required: 8,
    },
    {
      name: "Qwen3 14B",
      filename: "Qwen3-14B-Q4_K_M.gguf",
      url: "https://huggingface.co/Qwen/Qwen3-14B-GGUF/resolve/main/Qwen3-14B-Q4_K_M.gguf",
      size_label: "8.4 GB",
      ram_required: 16,
    },
    {
      name: "Qwen3 30B-A3B",
      filename: "Qwen3-30B-A3B-Q4_K_M.gguf",
      url: "https://huggingface.co/Qwen/Qwen3-30B-A3B-GGUF/resolve/main/Qwen3-30B-A3B-Q4_K_M.gguf",
      size_label: "18.6 GB",
      ram_required: 24,
    },
  ];

  const CURATED_STT_MODELS: CuratedSttModel[] = [
    {
      name: "Whisper Tiny",
      filename: "ggml-tiny.bin",
      url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.bin",
      repo: "ggerganov/whisper.cpp",
      size_label: "75 MB",
      ram_required: 1,
      quality_key: "onboarding.ai_setup.stt_quality_ultra_fast",
      lang_key: "onboarding.ai_setup.stt_lang_99",
    },
    {
      name: "Whisper Base",
      filename: "ggml-base.bin",
      url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin",
      repo: "ggerganov/whisper.cpp",
      size_label: "142 MB",
      ram_required: 2,
      quality_key: "onboarding.ai_setup.stt_quality_balanced",
      lang_key: "onboarding.ai_setup.stt_lang_99",
    },
    {
      name: "Whisper Large-v3 Turbo Q5",
      filename: "ggml-large-v3-turbo-q5_0.bin",
      url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo-q5_0.bin",
      repo: "ggerganov/whisper.cpp",
      size_label: "547 MB",
      ram_required: 4,
      quality_key: "onboarding.ai_setup.stt_quality_high_6x",
      lang_key: "onboarding.ai_setup.stt_lang_99",
    },
    {
      name: "Whisper Large-v3 Q5",
      filename: "ggml-large-v3-q5_0.bin",
      url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-q5_0.bin",
      repo: "ggerganov/whisper.cpp",
      size_label: "1.1 GB",
      ram_required: 8,
      quality_key: "onboarding.ai_setup.stt_quality_max_precision",
      lang_key: "onboarding.ai_setup.stt_lang_99",
    },
    {
      name: "Whisper Large-v3 French",
      filename: "ggml-model-q5_0.bin",
      url: "https://huggingface.co/bofenghuang/whisper-large-v3-french/resolve/main/ggml-model-q5_0.bin",
      repo: "bofenghuang/whisper-large-v3-french",
      size_label: "~1.1 GB",
      ram_required: 8,
      quality_key: "onboarding.ai_setup.stt_quality_french_tuned",
      lang_key: "onboarding.ai_setup.stt_lang_french",
    },
  ];

  // ─── State ────────────────────────────────────────────────────────────────

  let loading = $state(true);
  let sysInfo = $state<SystemInfo | null>(null);
  let ggufModels = $state<GgufModelInfo[]>([]);
  let whisperModels = $state<WhisperModelInfo[]>([]);

  let selectedGguf = $state<GgufModelInfo | null>(null);
  let llmConfiguring = $state(false);
  let llmSuccess = $state(false);
  let llmError = $state<string | null>(null);

  let sttEnabled = $state(false);
  let selectedWhisper = $state<WhisperModelInfo | null>(null);

  let llmDownloadId = $state<string | null>(null);
  let llmDownloadProgress = $state<DownloadProgress | null>(null);
  let llmDownloadingModel = $state<CuratedLlmModel | HfFile | null>(null);
  let llmDownloadError = $state<string | null>(null);

  let sttDownloadId = $state<string | null>(null);
  let sttDownloadProgress = $state<DownloadProgress | null>(null);
  let sttDownloadingModel = $state<CuratedSttModel | null>(null);
  let sttDownloadError = $state<string | null>(null);

  let showSearch = $state(false);
  let searchQuery = $state("");
  let searchLoading = $state(false);
  let searchResults = $state<HfModelCard[]>([]);
  let searchError = $state<string | null>(null);
  let expandedModel = $state<string | null>(null);
  let expandedDetail = $state<HfModelCard | null>(null);
  let expandLoading = $state(false);

  let advancing = $state(false);
  let advanceError = $state<string | null>(null);

  let importingLlm = $state(false);
  let importingStt = $state(false);

  // ── STT hotkey + live test ────────────────────────────────────────────
  // Lit la config courante (hotkey enregistré dans system.db), permet à
  // l'utilisateur de la définir au clavier (capture de touches) et de
  // tester le pipeline en direct sans quitter l'onboarding. La
  // transcription arrive via l'event Tauri `stt-transcribed`.
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

  onDestroy(() => {
    if (sttTestUnlisten !== null) {
      sttTestUnlisten();
      sttTestUnlisten = null;
    }
  });

  // ─── Derived ──────────────────────────────────────────────────────────────

  const availableLlmModels = $derived(
    CURATED_LLM_MODELS.filter((m) => !sysInfo || sysInfo.total_ram_gb >= m.ram_required),
  );

  const recommendedLlm = $derived.by((): CuratedLlmModel | null => {
    if (!sysInfo) return CURATED_LLM_MODELS[1] ?? null;
    const fitting = CURATED_LLM_MODELS.filter((m) => sysInfo!.total_ram_gb >= m.ram_required);
    return fitting[fitting.length - 1] ?? fitting[0] ?? null;
  });

  const availableSttModels = $derived(
    CURATED_STT_MODELS.filter((m) => !sysInfo || sysInfo.total_ram_gb >= m.ram_required),
  );

  const recommendedStt = $derived.by((): CuratedSttModel | null => {
    if (!sysInfo) return CURATED_STT_MODELS[2] ?? null;
    const turbo = CURATED_STT_MODELS.find((m) => m.filename.includes("turbo"));
    if (turbo && sysInfo!.total_ram_gb >= turbo.ram_required) return turbo;
    const fitting = CURATED_STT_MODELS.filter((m) => sysInfo!.total_ram_gb >= m.ram_required);
    return fitting[fitting.length - 1] ?? fitting[0] ?? null;
  });

  /**
   * The continue button should only enable once the runtime can actually
   * answer the agent's first turn - i.e. either the local LLM was wired up
   * during this session, or a backend (cloud or pre-existing local) is
   * already registered in the runtime.
   */
  const hasUsableLlm = $derived(llmSuccess || $llmBackends.length > 0);

  // ─── Effects ──────────────────────────────────────────────────────────────

  $effect(() => {
    void loadData();
  });

  $effect(() => {
    let unlisten: (() => void) | undefined;
    listen<DownloadProgress>("model-download-progress", (event) => {
      const p = event.payload;
      if (p.id === llmDownloadId) {
        llmDownloadProgress = p;
        if (p.status === "completed") {
          const downloadedFilename = llmDownloadingModel?.filename ?? null;
          llmDownloadId = null;
          llmDownloadingModel = null;
          void (async () => {
            await loadData();
            // Auto-wire the freshly downloaded model as the default LLM backend
            // so the user can chat immediately after onboarding. Without this,
            // the download only lands the .gguf on disk and no backend is ever
            // created in system.db, leaving the first chat with no model.
            if (downloadedFilename && !llmSuccess) {
              const model = ggufModels.find(
                (m) => m.filename === downloadedFilename,
              );
              if (model) await selectGgufModel(model);
            }
          })();
        } else if (p.status === "cancelled" || p.status === "failed") {
          llmDownloadId = null;
          llmDownloadingModel = null;
          llmDownloadProgress = null;
          if (p.status === "failed")
            llmDownloadError = get(t)("onboarding.ai_setup.download_failed");
        }
      } else if (p.id === sttDownloadId) {
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
      }
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      unlisten?.();
    };
  });

  // ─── Data loading ─────────────────────────────────────────────────────────

  async function loadData(): Promise<void> {
    try {
      const [sys, gguf, whisper, sttCfg] = await Promise.all([
        getAiSetupInfo(),
        scanForGgufModels(),
        scanForWhisperModels(),
        (
          getSttConfig() as Promise<{
            hotkey?: string;
            input_device?: string | null;
          }>
        ).catch(() => ({}) as { hotkey?: string; input_device?: string | null }),
      ]);
      sysInfo = sys;
      ggufModels = gguf;
      whisperModels = whisper;
      if (whisper.length > 0) {
        selectedWhisper = whisper[0];
        sttEnabled = whisper[0].recommended;
      }
      sttHotkey = sttCfg?.hotkey ?? "ctrl+shift+space";
      sttInputDevice = sttCfg?.input_device ?? "";
      void loadInputDevices();
    } catch {
      /* leave empty */
    } finally {
      loading = false;
    }
  }

  // ── STT hotkey + live test handlers ─────────────────────────────────────

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
        await setupWhisperModel(selectedWhisper.path, currentLocale());
      } else {
        await reloadStt();
      }
      await startTourRecording();
    } catch (err) {
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

  async function rescanStt(): Promise<void> {
    try {
      const whisper = await scanForWhisperModels();
      whisperModels = whisper;
      if (whisper.length > 0) {
        selectedWhisper = whisper[0];
        sttEnabled = whisper[0].recommended;
      }
    } catch {
      /* leave empty */
    }
  }

  // ─── LLM local model selection ────────────────────────────────────────────

  async function selectGgufModel(model: GgufModelInfo): Promise<void> {
    if (llmConfiguring || llmSuccess) return;
    selectedGguf = model;
    llmConfiguring = true;
    llmError = null;
    try {
      await setupLocalLlm(model.path);
      await reloadLlm();
      llmSuccess = true;
    } catch (err: unknown) {
      llmError = err instanceof Error ? err.message : String(err);
      llmConfiguring = false;
    }
  }

  // ─── Load a model from disk (file picker + copy into models dir) ───────────

  async function pickModelsDir(): Promise<string> {
    return pathJoin(await homeDir(), ".apollia", "models");
  }

  async function loadLlmModel(): Promise<void> {
    if (importingLlm || llmDownloadId) return;
    importingLlm = true;
    llmError = null;
    try {
      const selected = await openFilePicker({
        multiple: false,
        filters: [{ name: "GGUF", extensions: ["gguf"] }],
        title: get(t)("onboarding.ai_setup.load_model_title"),
        defaultPath: await pickModelsDir(),
      });
      if (!selected) return;
      const filePath =
        typeof selected === "string" ? selected : (selected as { path: string }).path;
      const dest = await importModelFile(filePath, ["gguf"]);
      await loadData();
      const name = dest.split(/[\\/]/).pop() ?? "";
      const model = ggufModels.find((m) => m.filename === name);
      if (model) await selectGgufModel(model);
    } catch (err: unknown) {
      llmError = err instanceof Error ? err.message : String(err);
    } finally {
      importingLlm = false;
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

  // ─── LLM download (curated) ───────────────────────────────────────────────

  /**
   * Extract `org/repo` from a HuggingFace direct-file URL.
   * Format observé : `https://huggingface.co/{org}/{repo}/resolve/{ref}/{path}`.
   * Retourne `null` pour toute URL non-HF - le backend traite alors le
   * download sans auto-persistance des sampling defaults.
   */
  function extractHfRepoId(url: string): string | null {
    try {
      const u = new URL(url);
      if (u.hostname !== "huggingface.co") return null;
      const parts = u.pathname.split("/").filter(Boolean);
      if (parts.length < 2) return null;
      return `${parts[0]}/${parts[1]}`;
    } catch {
      return null;
    }
  }

  async function downloadLlmModel(model: CuratedLlmModel): Promise<void> {
    if (llmDownloadId) return;
    llmDownloadError = null;
    llmDownloadProgress = null;
    llmDownloadingModel = model;
    try {
      const id = await startModelDownload({
        url: model.url,
        filename: model.filename,
        repo_id: extractHfRepoId(model.url),
      });
      llmDownloadId = id;
    } catch (err: unknown) {
      llmDownloadError = err instanceof Error ? err.message : String(err);
      llmDownloadingModel = null;
    }
  }

  async function cancelLlmDownload(): Promise<void> {
    if (!llmDownloadId) return;
    try {
      await cancelModelDownload(llmDownloadId);
    } catch {
      /* ignore */
    }
  }

  // ─── LLM HuggingFace search ───────────────────────────────────────────────

  async function searchHf(): Promise<void> {
    if (!searchQuery.trim() || searchLoading) return;
    searchLoading = true;
    searchError = null;
    searchResults = [];
    expandedModel = null;
    expandedDetail = null;
    try {
      const data = await searchHfModels(searchQuery.trim(), 8);
      searchResults = data.models;
    } catch (err: unknown) {
      searchError = err instanceof Error ? err.message : String(err);
    } finally {
      searchLoading = false;
    }
  }

  async function expandSearchModel(repoId: string): Promise<void> {
    if (expandedModel === repoId) {
      expandedModel = null;
      expandedDetail = null;
      return;
    }
    expandedModel = repoId;
    expandedDetail = null;
    expandLoading = true;
    try {
      expandedDetail = await getHfModel(repoId);
    } catch {
      /* show what we have */
    } finally {
      expandLoading = false;
    }
  }

  async function downloadHfFile(file: HfFile): Promise<void> {
    if (llmDownloadId) return;
    llmDownloadError = null;
    llmDownloadProgress = null;
    llmDownloadingModel = file;
    try {
      const id = await startModelDownload({
        url: file.download_url,
        filename: file.filename,
        repo_id: expandedDetail?.repo_id ?? extractHfRepoId(file.download_url),
      });
      llmDownloadId = id;
      showSearch = false;
    } catch (err: unknown) {
      llmDownloadError = err instanceof Error ? err.message : String(err);
      llmDownloadingModel = null;
    }
  }

  // ─── STT download ─────────────────────────────────────────────────────────

  async function downloadSttModel(model: CuratedSttModel): Promise<void> {
    if (sttDownloadId) return;
    sttDownloadError = null;
    sttDownloadProgress = null;
    sttDownloadingModel = model;
    try {
      const id = await startModelDownload({
        url: model.url,
        filename: model.filename,
      });
      sttDownloadId = id;
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

  // ─── Cloud backend redirect ───────────────────────────────────────────────

  function openCloudSettings(): void {
    navigateTo("llm");
    onopencloud();
  }

  // ─── Navigation ───────────────────────────────────────────────────────────

  async function handleContinue(): Promise<void> {
    if (advancing) return;
    advancing = true;
    advanceError = null;
    try {
      if (sttEnabled && selectedWhisper) {
        await setupWhisperModel(selectedWhisper.path, currentLocale());
      }
      onnext();
    } catch (err: unknown) {
      advanceError = err instanceof Error ? err.message : String(err);
      advancing = false;
    }
  }

  // ─── Helpers ──────────────────────────────────────────────────────────────

  function ramLabel(gb: number): string {
    return gb >= 1 ? `${Math.round(gb)} GB` : `${Math.round(gb * 1024)} MB`;
  }

  function osLabel(os: string): string {
    if (os === "macos") return "macOS";
    if (os === "linux") return "Linux";
    if (os === "windows") return "Windows";
    return os;
  }

  function dlBytes(p: DownloadProgress): string {
    const dl = (p.downloaded_bytes / 1e9).toFixed(2);
    if (!p.total_bytes) return `${dl} GB`;
    const total = (p.total_bytes / 1e9).toFixed(2);
    const pct = Math.round((p.downloaded_bytes / p.total_bytes) * 100);
    return `${dl} / ${total} GB · ${pct}%`;
  }

  function dlSpeed(bps: number): string {
    return bps >= 1e6 ? `${(bps / 1e6).toFixed(1)} MB/s` : `${Math.round(bps / 1000)} KB/s`;
  }

  function dlPct(p: DownloadProgress): number {
    if (!p.total_bytes) return 0;
    return Math.min(100, Math.round((p.downloaded_bytes / p.total_bytes) * 100));
  }

  function hfFileLabelKey(f: HfFile): string | null {
    if (f.compatibility === "fits") return "onboarding.ai_setup.compat_fits";
    if (f.compatibility === "might_fit") return "onboarding.ai_setup.compat_might_fit";
    if (f.compatibility === "too_large") return "onboarding.ai_setup.compat_too_large";
    return null;
  }
</script>

<div class="ai-setup" data-testid="onboarding-ai-setup">
  <header class="setup-header">
    <h2 class="setup-title">{$t("onboarding.ai_setup.title")}</h2>
    <p class="setup-subtitle">
      {$t("onboarding.ai_setup.subtitle")}
    </p>
  </header>

  {#if sysInfo}
    <div class="sys-info-bar" data-testid="system-info-bar">
      <span class="sys-chip">
        <MemoryStick size={11} strokeWidth={2} />{ramLabel(sysInfo.total_ram_gb)} RAM
      </span>
      <span class="sys-chip">
        <MonitorCog size={11} strokeWidth={2} />{osLabel(sysInfo.os)} · {sysInfo.arch}
      </span>
      {#if sysInfo.gpu_available}<span class="sys-chip sys-chip-gpu">GPU</span>{/if}
    </div>
  {/if}

  {#if loading}
    <div class="scan-loading" data-testid="scan-loading">
      <Spinner size={18} />
      <span>{$t("onboarding.ai_setup.scanning")}</span>
    </div>
  {:else}
    <!-- ─── LLM section ──────────────────────────────────────────────── -->
    <section class="setup-section" data-testid="llm-section">
      <div class="section-header">
        <HardDrive size={14} strokeWidth={2} class="text-primary" />
        <span class="section-title">{$t("onboarding.ai_setup.llm_section_title")}</span>
        {#if llmSuccess}
          <span class="section-badge-ok">
            <Check size={10} strokeWidth={2.5} /> {$t("onboarding.ai_setup.configured")}
          </span>
        {:else if $llmBackends.length > 0}
          <span class="section-badge-ok">
            <Check size={10} strokeWidth={2.5} /> {$t("onboarding.ai_setup.backends_count", { values: { count: $llmBackends.length } })}
          </span>
        {/if}
      </div>

      {#if ggufModels.length === 0 && !llmSuccess}
        <p class="empty-hint" data-testid="llm-empty-hint">
          {$t("onboarding.ai_setup.llm_empty_prefix")}
          <code>~/.apollia/models/</code> {$t("common.or")} <code>~/Downloads/</code>{$t("onboarding.ai_setup.llm_empty_suffix")}
          <Button variant="ghost" size="sm" class="inline-link" onclick={loadData}>{$t("onboarding.ai_setup.rescan")}</Button>.
        </p>

        <div class="load-model-row">
          <Button
            variant="default"
            size="sm"
            onclick={loadLlmModel}
            disabled={importingLlm || !!llmDownloadId}
            loading={importingLlm}
            data-testid="llm-load-model-btn"
          >
            <Upload size={12} strokeWidth={2} />
            {$t("onboarding.ai_setup.load_model")}
          </Button>
        </div>

        {#if llmDownloadId}
          <div class="download-block" data-testid="llm-download-progress">
            <div class="dl-header">
              <span class="dl-filename">
                {"filename" in (llmDownloadingModel ?? {})
                  ? (llmDownloadingModel as CuratedLlmModel | HfFile).filename
                  : "…"}
              </span>
              <Button variant="ghost" size="sm" class="btn-cancel-dl" onclick={cancelLlmDownload} aria-label={$t("onboarding.ai_setup.cancel_download")}>
                <X size={12} strokeWidth={2} />
              </Button>
            </div>
            <ProgressBar
              value={llmDownloadProgress ? dlPct(llmDownloadProgress) : undefined}
              size="sm"
              variant="primary"
            />
            {#if llmDownloadProgress}
              <div class="dl-meta">
                <span>{dlBytes(llmDownloadProgress)}</span>
                <span>{dlSpeed(llmDownloadProgress.speed_bps)}</span>
              </div>
            {/if}
          </div>
        {:else if showSearch}
          <div class="search-bar">
            <Button variant="ghost" size="sm" class="btn-back" onclick={() => { showSearch = false; searchResults = []; }} aria-label={$t("common.back")}>
              <ArrowLeft size={12} strokeWidth={2} />
            </Button>
            <Input
              class="search-input"
              type="text"
              placeholder={$t("onboarding.ai_setup.search_placeholder")}
              bind:value={searchQuery}
              onkeydown={(e) => e.key === "Enter" && searchHf()}
            />
            <Button variant="ghost" size="sm" class="btn-search" onclick={searchHf} disabled={searchLoading} aria-label={$t("onboarding.ai_setup.search")}>
              {#if searchLoading}<Spinner size={12} />{:else}<Search size={12} strokeWidth={2} />{/if}
            </Button>
          </div>

          {#if searchError}
            <p class="inline-error" role="alert"><AlertCircle size={12} />{searchError}</p>
          {/if}

          {#if searchResults.length > 0}
            <ul class="model-list" data-testid="search-results">
              {#each searchResults as model (model.repo_id)}
                {@const isExpanded = expandedModel === model.repo_id}
                {@const detail = isExpanded ? expandedDetail : null}
                <li class="search-result-item" class:is-expanded={isExpanded}>
                  <button
                    type="button"
                    class="search-result-header"
                    onclick={() => expandSearchModel(model.repo_id)}
                    disabled={model.compatibility_issue === "embedding_model" || model.compatibility_issue === "no_gguf_files"}
                  >
                    <div class="model-icon">
                      <Cpu size={12} strokeWidth={1.75} />
                    </div>
                    <span class="model-name">{model.repo_id}</span>
                    {#if model.gated}
                      <span class="badge-gated">{$t("onboarding.ai_setup.gated")}</span>
                    {/if}
                    <ChevronRight
                      size={12}
                      class="text-muted-foreground/50 transition-transform {isExpanded ? 'rotate-90' : ''}"
                    />
                  </button>

                  {#if isExpanded}
                    <div class="search-result-files">
                      {#if expandLoading && !detail}
                        <div class="files-loading"><Spinner size={12} /></div>
                      {:else}
                        {@const files = (detail ?? model).gguf_files.slice(0, 6)}
                        {#each files as file (file.filename)}
                          <button
                            class="file-row"
                            class:compat-fits={file.compatibility === "fits"}
                            class:compat-large={file.compatibility === "too_large"}
                            onclick={() => downloadHfFile(file)}
                            disabled={file.compatibility === "too_large"}
                          >
                            <Download size={10} strokeWidth={2} />
                            <span class="file-name">{file.filename}</span>
                            <span class="file-size">{file.size_human}</span>
                            {#if file.compatibility}
                              {@const compatKey = hfFileLabelKey(file)}
                              <span class="compat-chip compat-chip-{file.compatibility}">
                                {compatKey ? $t(compatKey) : ""}
                              </span>
                            {/if}
                          </button>
                        {/each}
                      {/if}
                    </div>
                  {/if}
                </li>
              {/each}
            </ul>
          {:else if !searchLoading}
            <p class="empty-hint" style="text-align:center">
              {searchQuery
                ? $t("onboarding.ai_setup.search_no_results")
                : $t("onboarding.ai_setup.search_prompt")}
            </p>
          {/if}
        {:else}
          <div class="curated-divider"><span>{$t("onboarding.ai_setup.recommended_models")}</span></div>
          <ul class="model-list" data-testid="curated-llm-list">
            {#each availableLlmModels as model (model.filename)}
              <li>
                <button type="button" class="model-row" onclick={() => downloadLlmModel(model)} data-testid="curated-llm-row">
                  <div class="model-icon"><Download size={12} strokeWidth={1.75} /></div>
                  <div class="model-info">
                    <span class="model-name">{model.name}</span>
                    <span class="model-meta">{model.size_label}</span>
                  </div>
                  {#if model.filename === recommendedLlm?.filename}
                    <span class="badge-recommended">{$t("onboarding.ai_setup.recommended")}</span>
                  {/if}
                  <ChevronRight size={13} class="text-muted-foreground/50" />
                </button>
              </li>
            {/each}
          </ul>
          <div class="alt-row">
            <Button variant="ghost" size="sm" class="btn-search-hf" onclick={() => { showSearch = true; searchResults = []; }}>
              <Search size={11} strokeWidth={2} />
              {$t("onboarding.ai_setup.search_hf")}
            </Button>
            <Button variant="ghost" size="sm" class="btn-search-hf btn-cloud" onclick={openCloudSettings} data-testid="onboarding-open-cloud">
              <Cloud size={11} strokeWidth={2} />
              {$t("onboarding.ai_setup.use_cloud")}
            </Button>
          </div>
        {/if}

        {#if llmDownloadError}
          <p class="inline-error" role="alert" data-testid="llm-download-error">
            <AlertCircle size={12} />{llmDownloadError}
          </p>
        {/if}
      {:else if llmSuccess}
        <div class="success-row" data-testid="llm-success">
          <div class="success-icon-sm"><Check size={13} strokeWidth={2.5} /></div>
          <span class="success-filename">{selectedGguf?.filename}</span>
        </div>
      {:else}
        <ul class="model-list" data-testid="llm-model-list">
          {#each ggufModels as model (model.path)}
            <li>
              <button
                class="model-row"
                class:is-selected={selectedGguf?.path === model.path && llmConfiguring}
                onclick={() => selectGgufModel(model)}
                disabled={llmConfiguring || llmSuccess}
                data-testid="llm-model-row"
              >
                <div class="model-icon">
                  {#if selectedGguf?.path === model.path && llmConfiguring}
                    <Spinner size={13} />
                  {:else}
                    <Cpu size={13} strokeWidth={1.75} />
                  {/if}
                </div>
                <div class="model-info">
                  <span class="model-name">{model.filename}</span>
                  <span class="model-meta">{model.size_human}</span>
                </div>
                {#if model.recommended}
                  <span class="badge-recommended">{$t("onboarding.ai_setup.recommended")}</span>
                {/if}
                <ChevronRight size={13} class="text-muted-foreground/50" />
              </button>
            </li>
          {/each}
        </ul>
        {#if llmError}
          <p class="inline-error" role="alert" data-testid="llm-error"><AlertCircle size={12} />{llmError}</p>
        {/if}
      {/if}
    </section>

    <!-- ─── STT section ──────────────────────────────────────────────── -->
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

      {#if whisperModels.length === 0}
        <p class="empty-hint" data-testid="stt-empty-hint">
          {$t("onboarding.ai_setup.stt_empty_hint")}
        </p>

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
        {:else}
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
      {:else if sttEnabled}
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
  {/if}

  {#if advanceError}
    <p class="inline-error" role="alert" data-testid="advance-error">
      <AlertCircle size={12} />{advanceError}
    </p>
  {/if}

  <footer class="setup-footer">
    <Button variant="ghost" size="sm" class="btn-secondary" onclick={onback} disabled={advancing} data-testid="ai-setup-back">
      ← {$t("common.back")}
    </Button>
    <div class="footer-right">
      <Button variant="ghost" size="sm" class="btn-tertiary" onclick={onskip} disabled={advancing} data-testid="ai-setup-skip">
        {$t("onboarding.ai_setup.configure_later")}
      </Button>
      <Button
        variant="primary-gradient"
        size="default"
        onclick={handleContinue}
        disabled={advancing || loading || !hasUsableLlm}
        loading={advancing}
        data-testid="ai-setup-continue"
      >
        {$t("common.continue")}
      </Button>
    </div>
  </footer>
</div>

<HotkeyCaptureDialog
  open={sttHotkeyCapturing}
  onconfirm={onHotkeyConfirm}
  oncancel={onHotkeyCancel}
/>

<style>
  .ai-setup {
    display: flex;
    flex-direction: column;
    gap: 1rem;
  }

  /* Header */
  .setup-header {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.375rem;
    text-align: center;
  }
  .setup-title {
    font-size: 1.25rem;
    font-weight: 600;
    color: hsl(var(--foreground));
    margin: 0;
    letter-spacing: -0.015em;
  }
  .setup-subtitle {
    font-size: 0.8125rem;
    color: hsl(var(--muted-foreground));
    margin: 0;
    line-height: 1.5;
  }

  /* Sys chips */
  .sys-info-bar {
    display: flex;
    gap: 0.4rem;
    flex-wrap: wrap;
    justify-content: center;
  }
  .sys-chip {
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
    font-size: 0.6875rem;
    font-weight: 500;
    color: hsl(var(--muted-foreground));
    background: hsl(var(--muted));
    padding: 0.2rem 0.55rem;
    border-radius: 99px;
  }
  .sys-chip-gpu {
    color: hsl(var(--secondary));
    background: hsl(var(--secondary) / 0.12);
  }

  /* Scan loading */
  .scan-loading {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 0.5rem;
    padding: 1.25rem 0;
    font-size: 0.8125rem;
    color: hsl(var(--muted-foreground));
  }

  /* Section */
  .setup-section {
    background: hsl(var(--muted) / 0.4);
    border: 1px solid hsl(var(--border));
    border-radius: 0.75rem;
    padding: 0.875rem;
    display: flex;
    flex-direction: column;
    gap: 0.625rem;
  }
  .section-header {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }
  .section-title {
    flex: 1;
    font-size: 0.8125rem;
    font-weight: 600;
    color: hsl(var(--foreground));
  }
  .section-badge-ok {
    display: inline-flex;
    align-items: center;
    gap: 0.25rem;
    font-size: 0.6875rem;
    font-weight: 600;
    color: hsl(var(--success));
    background: hsl(var(--success) / 0.12);
    padding: 0.15rem 0.45rem;
    border-radius: 99px;
  }

  /* STT hotkey + test */
  .stt-hotkey-block {
    margin-top: 0.75rem;
    padding-top: 0.75rem;
    border-top: 1px dashed hsl(var(--border) / 0.5);
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }
  .hotkey-row {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }
  .hotkey-label {
    font-size: 0.75rem;
    color: hsl(var(--muted-foreground));
    flex-shrink: 0;
  }
  .hotkey-capture {
    flex: 1;
    min-width: 0;
    padding: 0.25rem 0.5rem;
    border-radius: 0.375rem;
    border: 1px dashed hsl(var(--border));
    background: hsl(var(--background));
    font-size: 0.75rem;
    color: hsl(var(--muted-foreground));
    text-align: left;
    cursor: pointer;
    transition: all 150ms ease;
  }
  .hotkey-capture:hover {
    border-color: hsl(var(--primary) / 0.6);
  }
  .hotkey-capture:focus {
    outline: none;
    border-style: solid;
    border-color: hsl(var(--primary));
    box-shadow: 0 0 0 2px hsl(var(--primary) / 0.2);
  }
  .hotkey-display {
    font-family:
      ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    font-size: 0.75rem;
    color: hsl(var(--foreground));
    padding: 0.0625rem 0.375rem;
    border-radius: 0.25rem;
    background: hsl(var(--muted) / 0.6);
  }
  .hotkey-placeholder {
    font-style: italic;
    opacity: 0.7;
  }
  .hotkey-save {
    padding: 0.25rem 0.625rem;
    border-radius: 0.375rem;
    border: 1px solid hsl(var(--border));
    background: hsl(var(--card));
    font-size: 0.75rem;
    color: hsl(var(--foreground));
  }
  .hotkey-save:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .hotkey-save:not(:disabled):hover {
    background: hsl(var(--muted));
  }
  .hotkey-hint {
    margin: 0;
    font-size: 0.6875rem;
    color: hsl(var(--muted-foreground));
  }
  .mic-select {
    flex: 1;
    min-width: 0;
    padding: 0.25rem 0.5rem;
    border-radius: 0.375rem;
    border: 1px solid hsl(var(--border));
    background: hsl(var(--background));
    font-size: 0.75rem;
    color: hsl(var(--foreground));
    cursor: pointer;
  }
  .mic-select:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .stt-no-mic {
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
    margin: 0;
    padding: 0.35rem 0.5rem;
    border-radius: 0.375rem;
    border: 1px solid hsl(var(--warning) / 0.3);
    background: hsl(var(--warning) / 0.1);
    font-size: 0.6875rem;
    color: hsl(var(--warning-foreground));
  }
  .stt-test-row {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    flex-wrap: wrap;
  }
  .stt-test-btn {
    display: inline-flex;
    align-items: center;
    gap: 0.25rem;
    padding: 0.25rem 0.625rem;
    border-radius: 0.375rem;
    border: 1px solid hsl(var(--border));
    background: hsl(var(--card));
    font-size: 0.75rem;
    color: hsl(var(--foreground));
  }
  .stt-test-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .stt-test-stop {
    background: hsl(var(--destructive) / 0.15);
    color: hsl(var(--destructive));
    border-color: hsl(var(--destructive) / 0.3);
    animation: mic-pulse 1.4s ease-out infinite;
  }
  @keyframes mic-pulse {
    0% {
      box-shadow: 0 0 0 0 hsl(var(--destructive) / 0.45);
    }
    70% {
      box-shadow: 0 0 0 6px hsl(var(--destructive) / 0);
    }
    100% {
      box-shadow: 0 0 0 0 hsl(var(--destructive) / 0);
    }
  }
  .stt-test-transcript {
    display: inline-flex;
    align-items: center;
    gap: 0.25rem;
    font-size: 0.75rem;
    color: hsl(var(--success));
    font-style: italic;
  }
  .stt-test-error {
    display: inline-flex;
    align-items: center;
    gap: 0.25rem;
    font-size: 0.75rem;
    color: hsl(var(--destructive));
  }

  /* Model list */
  .model-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
  }
  .model-row {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    width: 100%;
    padding: 0.5rem 0.625rem;
    border-radius: 0.5rem;
    border: 1px solid hsl(var(--border));
    background: hsl(var(--card));
    cursor: pointer;
    text-align: left;
    transition:
      background 120ms ease,
      border-color 120ms ease;
  }
  .model-row:hover:not(:disabled) {
    background: hsl(var(--primary) / 0.04);
    border-color: hsl(var(--primary) / 0.25);
  }
  .model-row:disabled {
    opacity: 0.55;
    cursor: not-allowed;
  }
  .model-row.is-selected {
    border-color: hsl(var(--primary) / 0.4);
    background: hsl(var(--primary) / 0.04);
  }

  .model-icon {
    width: 1.625rem;
    height: 1.625rem;
    border-radius: 0.375rem;
    background: hsl(var(--primary) / 0.1);
    display: flex;
    align-items: center;
    justify-content: center;
    color: hsl(var(--primary));
    flex-shrink: 0;
  }
  .model-icon-stt {
    background: hsl(var(--secondary) / 0.12);
    color: hsl(var(--secondary));
  }

  .model-info {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 0.05rem;
    min-width: 0;
  }
  .model-name {
    font-size: 0.8125rem;
    font-weight: 500;
    color: hsl(var(--foreground));
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .model-meta {
    font-size: 0.6875rem;
    color: hsl(var(--muted-foreground));
  }
  .model-lang {
    font-weight: 500;
  }
  .capitalize {
    text-transform: capitalize;
  }

  .badge-recommended {
    font-size: 0.6rem;
    font-weight: 700;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: hsl(var(--primary));
    background: hsl(var(--primary) / 0.1);
    padding: 0.18rem 0.4rem;
    border-radius: 99px;
    flex-shrink: 0;
  }
  .badge-recommended-stt {
    color: hsl(var(--secondary));
    background: hsl(var(--secondary) / 0.12);
  }

  /* Success */
  .success-row {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }
  .success-icon-sm {
    width: 1.375rem;
    height: 1.375rem;
    border-radius: 50%;
    background: hsl(var(--success));
    color: hsl(var(--primary-foreground));
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
  }
  .success-filename {
    font-size: 0.8125rem;
    font-weight: 500;
    color: hsl(var(--success));
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  /* Rescan */
  .btn-rescan {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 1.375rem;
    height: 1.375rem;
    border-radius: 0.375rem;
    border: 1px solid hsl(var(--border));
    background: transparent;
    color: hsl(var(--muted-foreground));
    cursor: pointer;
    flex-shrink: 0;
    transition:
      background 120ms ease,
      color 120ms ease;
  }
  .btn-rescan:hover {
    background: hsl(var(--secondary) / 0.08);
    color: hsl(var(--secondary));
  }

  /* Toggle */
  .toggle-label {
    display: flex;
    align-items: center;
    cursor: pointer;
    flex-shrink: 0;
  }
  .toggle-input {
    position: absolute;
    opacity: 0;
    width: 0;
    height: 0;
  }
  .toggle-track {
    width: 2rem;
    height: 1.125rem;
    border-radius: 99px;
    background: hsl(var(--muted));
    border: 1px solid hsl(var(--border));
    position: relative;
    transition: background 150ms ease;
  }
  .toggle-track::after {
    content: "";
    position: absolute;
    top: 0.125rem;
    left: 0.125rem;
    width: 0.75rem;
    height: 0.75rem;
    border-radius: 50%;
    background: hsl(var(--primary-foreground));
    transition: transform 150ms ease;
    box-shadow: var(--shadow-elev-1);
  }
  .toggle-track.on {
    background: hsl(var(--secondary));
    border-color: hsl(var(--secondary));
  }
  .toggle-track.on::after {
    transform: translateX(0.875rem);
  }

  /* Empty hints */
  .empty-hint {
    font-size: 0.8125rem;
    color: hsl(var(--muted-foreground));
    margin: 0;
    line-height: 1.5;
  }
  .empty-hint code {
    font-family:
      ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    font-size: 0.85em;
    background: hsl(var(--muted));
    padding: 0.1em 0.3em;
    border-radius: 0.25rem;
    color: hsl(var(--foreground) / 0.85);
  }
  .inline-link {
    background: none;
    border: none;
    padding: 0;
    color: hsl(var(--primary));
    cursor: pointer;
    font-size: inherit;
    text-decoration: underline;
    text-underline-offset: 2px;
  }

  /* Curated divider */
  .curated-divider {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }
  .curated-divider::before,
  .curated-divider::after {
    content: "";
    flex: 1;
    height: 1px;
    background: hsl(var(--border));
  }
  .curated-divider span {
    font-size: 0.6875rem;
    color: hsl(var(--muted-foreground));
    white-space: nowrap;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    font-weight: 500;
  }

  /* Load a model from disk */
  .load-model-row {
    display: flex;
    justify-content: center;
  }

  /* Alt row */
  .alt-row {
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
    align-items: stretch;
  }
  @media (min-width: 540px) {
    .alt-row {
      flex-direction: row;
      justify-content: center;
    }
  }
  .btn-search-hf {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 0.375rem;
    padding: 0.35rem 0.75rem;
    border: 1px solid hsl(var(--border));
    border-radius: 0.5rem;
    background: hsl(var(--card));
    color: hsl(var(--muted-foreground));
    font-size: 0.75rem;
    font-weight: 500;
    cursor: pointer;
    transition:
      background 120ms ease,
      color 120ms ease,
      border-color 120ms ease;
  }
  .btn-search-hf:hover {
    background: hsl(var(--muted));
    color: hsl(var(--foreground));
    border-color: hsl(var(--primary) / 0.25);
  }
  .btn-cloud:hover {
    color: hsl(var(--secondary));
    border-color: hsl(var(--secondary) / 0.35);
  }

  /* Search bar */
  .search-bar {
    display: flex;
    gap: 0.375rem;
    align-items: center;
  }
  .btn-back {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 1.875rem;
    height: 1.875rem;
    border-radius: 0.4rem;
    border: 1px solid hsl(var(--border));
    background: transparent;
    color: hsl(var(--muted-foreground));
    cursor: pointer;
    flex-shrink: 0;
    transition: background 120ms ease;
  }
  .btn-back:hover {
    background: hsl(var(--muted));
  }
  .search-input {
    flex: 1;
    height: 1.875rem;
    padding: 0 0.625rem;
    border-radius: 0.4rem;
    border: 1px solid hsl(var(--border));
    background: hsl(var(--card));
    color: hsl(var(--foreground));
    font-size: 0.8125rem;
    outline: none;
    transition: border-color 120ms ease;
  }
  .search-input:focus {
    border-color: hsl(var(--primary) / 0.5);
  }
  .btn-search {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 1.875rem;
    height: 1.875rem;
    border-radius: 0.4rem;
    border: none;
    background: hsl(var(--primary) / 0.12);
    color: hsl(var(--primary));
    cursor: pointer;
    flex-shrink: 0;
    transition: background 120ms ease;
  }
  .btn-search:hover:not(:disabled) {
    background: hsl(var(--primary) / 0.2);
  }
  .btn-search:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  /* Search results */
  .search-result-item {
    border: 1px solid hsl(var(--border));
    border-radius: 0.5rem;
    overflow: hidden;
    background: hsl(var(--card));
  }
  .search-result-item.is-expanded {
    border-color: hsl(var(--primary) / 0.3);
  }
  .search-result-header {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    width: 100%;
    padding: 0.45rem 0.625rem;
    background: none;
    border: none;
    cursor: pointer;
    text-align: left;
    transition: background 120ms ease;
  }
  .search-result-header:hover:not(:disabled) {
    background: hsl(var(--primary) / 0.04);
  }
  .search-result-header:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }
  .badge-gated {
    font-size: 0.6rem;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: hsl(var(--warning));
    background: hsl(var(--warning) / 0.12);
    padding: 0.15rem 0.4rem;
    border-radius: 99px;
    flex-shrink: 0;
  }
  .search-result-files {
    padding: 0 0.625rem 0.625rem;
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }
  .files-loading {
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 0.5rem 0;
  }
  .file-row {
    display: flex;
    align-items: center;
    gap: 0.375rem;
    padding: 0.35rem 0.5rem;
    border-radius: 0.35rem;
    border: 1px solid transparent;
    background: hsl(var(--muted) / 0.5);
    cursor: pointer;
    text-align: left;
    font-size: 0.75rem;
    color: hsl(var(--foreground));
    transition:
      background 100ms ease,
      border-color 100ms ease;
  }
  .file-row:hover:not(:disabled) {
    background: hsl(var(--primary) / 0.06);
    border-color: hsl(var(--primary) / 0.18);
  }
  .file-row:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }
  .file-row.compat-fits {
    border-color: hsl(var(--success) / 0.25);
  }
  .file-row.compat-large {
    opacity: 0.4;
  }
  .file-name {
    flex: 1;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .file-size {
    font-size: 0.6875rem;
    color: hsl(var(--muted-foreground));
    flex-shrink: 0;
  }
  .compat-chip {
    font-size: 0.6rem;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.03em;
    padding: 0.1rem 0.35rem;
    border-radius: 99px;
    flex-shrink: 0;
  }
  .compat-chip-fits {
    color: hsl(var(--success));
    background: hsl(var(--success) / 0.12);
  }
  .compat-chip-might_fit {
    color: hsl(var(--warning));
    background: hsl(var(--warning) / 0.12);
  }
  .compat-chip-too_large {
    color: hsl(var(--destructive));
    background: hsl(var(--destructive) / 0.1);
  }

  /* Download block */
  .download-block {
    display: flex;
    flex-direction: column;
    gap: 0.375rem;
    padding: 0.55rem 0.7rem;
    border-radius: 0.5rem;
    border: 1px solid hsl(var(--primary) / 0.2);
    background: hsl(var(--primary) / 0.05);
  }
  .dl-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
  }
  .dl-filename {
    font-size: 0.8125rem;
    font-weight: 500;
    color: hsl(var(--foreground));
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    flex: 1;
  }
  .btn-cancel-dl {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 1.25rem;
    height: 1.25rem;
    border-radius: 50%;
    border: none;
    background: hsl(var(--muted));
    color: hsl(var(--muted-foreground));
    cursor: pointer;
    flex-shrink: 0;
    transition:
      background 120ms ease,
      color 120ms ease;
  }
  .btn-cancel-dl:hover {
    background: hsl(var(--destructive) / 0.12);
    color: hsl(var(--destructive));
  }
  .progress-track {
    height: 0.25rem;
    border-radius: 99px;
    background: hsl(var(--primary) / 0.15);
    overflow: hidden;
  }
  .progress-fill {
    height: 100%;
    border-radius: 99px;
    background: linear-gradient(90deg, hsl(var(--primary)), hsl(var(--secondary)));
    transition: width 300ms ease;
  }
  .progress-fill.indeterminate {
    width: 40%;
    animation: indeterminate 1.4s ease-in-out infinite;
  }
  @keyframes indeterminate {
    0% {
      transform: translateX(-100%);
    }
    100% {
      transform: translateX(350%);
    }
  }
  .dl-meta {
    display: flex;
    justify-content: space-between;
    font-size: 0.6875rem;
    color: hsl(var(--muted-foreground));
  }

  /* Errors */
  .inline-error {
    display: flex;
    align-items: center;
    gap: 0.375rem;
    font-size: 0.8125rem;
    color: hsl(var(--destructive));
    background: hsl(var(--destructive) / 0.05);
    border: 1px solid hsl(var(--destructive) / 0.2);
    border-radius: 0.4rem;
    padding: 0.45rem 0.625rem;
    margin: 0;
  }

  /* Footer */
  .setup-footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
    padding-top: 0.5rem;
    border-top: 1px solid hsl(var(--border));
  }
  .footer-right {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }
  .btn-secondary,
  .btn-tertiary {
    background: none;
    border: none;
    color: hsl(var(--muted-foreground));
    font-size: 0.8125rem;
    cursor: pointer;
    padding: 0.4rem 0.6rem;
    border-radius: 0.4rem;
    transition:
      color 150ms ease,
      background 150ms ease;
  }
  .btn-secondary:hover:not(:disabled),
  .btn-tertiary:hover:not(:disabled) {
    color: hsl(var(--foreground));
    background: hsl(var(--muted));
  }
  .btn-secondary:disabled,
  .btn-tertiary:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
</style>
