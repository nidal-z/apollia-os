<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import {
    Cpu,
    HardDrive,
    Mic,
    Check,
    ChevronRight,
    Loader2,
    AlertCircle,
    Sparkles,
    MemoryStick,
    MonitorCog,
  } from "lucide-svelte";
  import { t } from "svelte-i18n";
  import { onboardingStore } from "$lib/stores/onboarding";

  interface SystemInfo {
    total_ram_gb: number;
    available_ram_gb: number;
    os: string;
    arch: string;
    gpu_available: boolean;
  }

  interface GgufModelInfo {
    path: string;
    filename: string;
    size_bytes: number;
    size_human: string;
    recommended: boolean;
  }

  interface WhisperModelInfo {
    path: string;
    filename: string;
    size_bytes: number;
    model_size: string;
    recommended: boolean;
  }

  let visible = $state(false);
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

  let advancing = $state(false);
  let advanceError = $state<string | null>(null);

  $effect(() => {
    requestAnimationFrame(() => {
      visible = true;
    });
    void loadData();
  });

  async function loadData(): Promise<void> {
    try {
      const [sys, gguf, whisper] = await Promise.all([
        invoke<SystemInfo>("get_ai_setup_info"),
        invoke<GgufModelInfo[]>("scan_for_gguf_models"),
        invoke<WhisperModelInfo[]>("scan_for_whisper_models"),
      ]);
      sysInfo = sys;
      ggufModels = gguf;
      whisperModels = whisper;
      if (whisper.length > 0) {
        selectedWhisper = whisper[0];
        sttEnabled = whisper[0].recommended;
      }
    } catch {
      // leave models empty — UI shows the help message
    } finally {
      loading = false;
    }
  }

  async function selectGgufModel(model: GgufModelInfo): Promise<void> {
    if (llmConfiguring || llmSuccess) return;
    selectedGguf = model;
    llmConfiguring = true;
    llmError = null;
    try {
      await invoke("setup_local_llm", { ggufPath: model.path });
      await invoke("reload_llm");
      llmSuccess = true;
    } catch (err: unknown) {
      llmError = err instanceof Error ? err.message : String(err);
      llmConfiguring = false;
    }
  }

  async function handleContinue(): Promise<void> {
    if (advancing) return;
    advancing = true;
    advanceError = null;
    try {
      if (sttEnabled && selectedWhisper) {
        await invoke("setup_whisper_model", { modelPath: selectedWhisper.path });
      }
      await onboardingStore.advancePhase("acquaintance");
    } catch (err: unknown) {
      advanceError = err instanceof Error ? err.message : String(err);
      advancing = false;
    }
  }

  function ramLabel(gb: number): string {
    return gb >= 1 ? `${Math.round(gb)} GB` : `${Math.round(gb * 1024)} MB`;
  }

  function osLabel(os: string): string {
    if (os === "macos") return "macOS";
    if (os === "linux") return "Linux";
    if (os === "windows") return "Windows";
    return os;
  }
</script>

<div class="ai-setup-screen" class:visible data-testid="onboarding-ai-setup">
  <div class="ai-setup-content">

    <!-- Header -->
    <header class="ai-setup-header">
      <div class="ai-setup-logo">
        <Sparkles size={26} strokeWidth={1.5} class="text-white" />
      </div>
      <h1 class="ai-setup-title">{$t("onboarding_v2.ai_setup.title")}</h1>
      <p class="ai-setup-subtitle">{$t("onboarding_v2.ai_setup.subtitle")}</p>
    </header>

    <!-- System info bar -->
    {#if sysInfo}
      <div class="sys-info-bar" data-testid="system-info-bar">
        <span class="sys-chip">
          <MemoryStick size={12} strokeWidth={2} />
          {ramLabel(sysInfo.total_ram_gb)} RAM
        </span>
        <span class="sys-chip">
          <MonitorCog size={12} strokeWidth={2} />
          {osLabel(sysInfo.os)} · {sysInfo.arch}
        </span>
        {#if sysInfo.gpu_available}
          <span class="sys-chip sys-chip-gpu">GPU</span>
        {/if}
      </div>
    {/if}

    {#if loading}
      <div class="scan-loading" data-testid="scan-loading">
        <Loader2 size={20} class="animate-spin text-indigo-500" />
        <span>{$t("onboarding_v2.ai_setup.scanning")}</span>
      </div>
    {:else}
      <!-- LLM section -->
      <section class="setup-section" data-testid="llm-section">
        <div class="section-header">
          <HardDrive size={15} strokeWidth={2} class="text-indigo-500" />
          <span class="section-title">{$t("onboarding_v2.ai_setup.llm_section.title")}</span>
          {#if llmSuccess}
            <span class="section-badge-ok">
              <Check size={11} strokeWidth={2.5} /> {$t("onboarding_v2.ai_setup.llm_section.configured")}
            </span>
          {/if}
        </div>

        {#if ggufModels.length === 0}
          <p class="empty-hint" data-testid="llm-empty-hint">
            {$t("onboarding_v2.ai_setup.llm_section.no_models")}
            <code>~/.apollia/models/</code> ou <code>~/Downloads/</code>, puis
            <button class="inline-link" onclick={loadData}>{$t("onboarding_v2.ai_setup.llm_section.rescan")}</button>.
          </p>
        {:else if llmSuccess}
          <div class="success-row" data-testid="llm-success">
            <div class="success-icon-sm">
              <Check size={14} strokeWidth={2.5} class="text-white" />
            </div>
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
                      <Loader2 size={14} class="animate-spin" />
                    {:else}
                      <Cpu size={14} strokeWidth={1.75} />
                    {/if}
                  </div>
                  <div class="model-info">
                    <span class="model-name">{model.filename}</span>
                    <span class="model-meta">{model.size_human}</span>
                  </div>
                  {#if model.recommended}
                    <span class="badge-recommended">{$t("onboarding_v2.ai_setup.llm_section.recommended")}</span>
                  {/if}
                  <ChevronRight size={14} class="text-gray-300" />
                </button>
              </li>
            {/each}
          </ul>
          {#if llmError}
            <p class="inline-error" role="alert" data-testid="llm-error">
              <AlertCircle size={13} />{llmError}
            </p>
          {/if}
        {/if}
      </section>

      <!-- STT section -->
      <section class="setup-section" data-testid="stt-section">
        <div class="section-header">
          <Mic size={15} strokeWidth={2} class="text-violet-500" />
          <span class="section-title">{$t("onboarding_v2.ai_setup.stt_section.title")}</span>
          <label class="toggle-label">
            <input
              type="checkbox"
              class="toggle-input"
              bind:checked={sttEnabled}
              disabled={whisperModels.length === 0}
              data-testid="stt-toggle"
            />
            <span class="toggle-track" class:on={sttEnabled}></span>
          </label>
        </div>

        {#if whisperModels.length === 0}
          <p class="empty-hint" data-testid="stt-empty-hint">
            {$t("onboarding_v2.ai_setup.llm_section.no_whisper")}
          </p>
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
                  <div class="model-icon model-icon-stt">
                    <Mic size={13} strokeWidth={1.75} />
                  </div>
                  <div class="model-info">
                    <span class="model-name">{model.filename}</span>
                    <span class="model-meta capitalize">{model.model_size}</span>
                  </div>
                  {#if model.recommended}
                    <span class="badge-recommended badge-recommended-stt">{$t("onboarding_v2.ai_setup.stt_section.recommended")}</span>
                  {/if}
                  {#if selectedWhisper?.path === model.path}
                    <Check size={14} strokeWidth={2.5} class="text-violet-500" />
                  {:else}
                    <ChevronRight size={14} class="text-gray-300" />
                  {/if}
                </button>
              </li>
            {/each}
          </ul>
        {/if}
      </section>
    {/if}

    <!-- Error -->
    {#if advanceError}
      <p class="inline-error" role="alert" data-testid="advance-error">
        <AlertCircle size={13} />{advanceError}
      </p>
    {/if}

    <!-- Footer actions -->
    <footer class="ai-setup-footer">
      <button
        class="btn-continue"
        onclick={handleContinue}
        disabled={advancing || loading}
        data-testid="ai-setup-continue"
      >
        {#if advancing}
          <Loader2 size={15} class="animate-spin" />
          {$t("onboarding_v2.ai_setup.loading")}
        {:else}
          {$t("onboarding_v2.ai_setup.continue")}
          <ChevronRight size={15} strokeWidth={2} />
        {/if}
      </button>
      <button
        class="btn-skip"
        onclick={() => onboardingStore.advancePhase("acquaintance")}
        disabled={advancing}
        data-testid="ai-setup-skip"
      >
        {$t("onboarding_v2.ai_setup.configure_later")}
      </button>
    </footer>

  </div>
</div>

<style>
  .ai-setup-screen {
    position: fixed;
    inset: 0;
    z-index: 50;
    display: flex;
    align-items: flex-start;
    justify-content: center;
    background: #FFF8F0;
    overflow-y: auto;
    opacity: 0;
    transition: opacity 300ms ease-in;
  }

  .ai-setup-screen.visible {
    opacity: 1;
  }

  .ai-setup-content {
    width: 100%;
    max-width: 30rem;
    padding: 2.5rem 1.25rem 2rem;
    display: flex;
    flex-direction: column;
    gap: 1.25rem;
  }

  /* Header */
  .ai-setup-header {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.5rem;
    text-align: center;
  }

  .ai-setup-logo {
    width: 3.25rem;
    height: 3.25rem;
    border-radius: 1rem;
    background: linear-gradient(135deg, #3435f5, #7c5fd6);
    display: flex;
    align-items: center;
    justify-content: center;
    box-shadow: 0 4px 16px -2px rgba(52, 53, 245, 0.25);
    margin-bottom: 0.25rem;
  }

  .ai-setup-title {
    font-size: 1.5rem;
    font-weight: 700;
    color: #1a1a2e;
    margin: 0;
    letter-spacing: -0.02em;
  }

  .ai-setup-subtitle {
    font-size: 0.8125rem;
    color: #6B7280;
    margin: 0;
    line-height: 1.55;
  }

  /* System info bar */
  .sys-info-bar {
    display: flex;
    gap: 0.5rem;
    flex-wrap: wrap;
    justify-content: center;
  }

  .sys-chip {
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
    font-size: 0.6875rem;
    font-weight: 500;
    color: #6B7280;
    background: rgba(107, 114, 128, 0.08);
    padding: 0.25rem 0.625rem;
    border-radius: 99px;
  }

  .sys-chip-gpu {
    color: #7c5fd6;
    background: rgba(124, 95, 214, 0.1);
  }

  /* Loading */
  .scan-loading {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 0.625rem;
    padding: 1.5rem 0;
    font-size: 0.875rem;
    color: #6B7280;
  }

  /* Sections */
  .setup-section {
    background: rgba(255, 255, 255, 0.72);
    border: 1px solid rgba(52, 53, 245, 0.07);
    border-radius: 1rem;
    padding: 1rem;
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
    box-shadow:
      inset 0 0 0 1px rgba(255, 255, 255, 0.5),
      0 1px 4px rgba(0, 0, 0, 0.03);
  }

  .section-header {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .section-title {
    flex: 1;
    font-size: 0.875rem;
    font-weight: 600;
    color: #374151;
  }

  .section-badge-ok {
    display: inline-flex;
    align-items: center;
    gap: 0.25rem;
    font-size: 0.6875rem;
    font-weight: 600;
    color: #16a34a;
    background: rgba(34, 197, 94, 0.1);
    padding: 0.2rem 0.5rem;
    border-radius: 99px;
  }

  /* Model lists */
  .model-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.375rem;
  }

  .model-row {
    display: flex;
    align-items: center;
    gap: 0.625rem;
    width: 100%;
    padding: 0.625rem 0.75rem;
    border-radius: 0.625rem;
    border: 1px solid rgba(52, 53, 245, 0.07);
    background: rgba(255, 255, 255, 0.6);
    cursor: pointer;
    text-align: left;
    transition: background 120ms ease, border-color 120ms ease;
  }

  .model-row:hover:not(:disabled) {
    background: rgba(52, 53, 245, 0.04);
    border-color: rgba(52, 53, 245, 0.15);
  }

  .model-row:disabled {
    opacity: 0.55;
    cursor: not-allowed;
  }

  .model-row.is-selected {
    border-color: rgba(52, 53, 245, 0.3);
    background: rgba(52, 53, 245, 0.04);
  }

  .model-icon {
    width: 1.75rem;
    height: 1.75rem;
    border-radius: 0.4rem;
    background: rgba(52, 53, 245, 0.08);
    display: flex;
    align-items: center;
    justify-content: center;
    color: #3435f5;
    flex-shrink: 0;
  }

  .model-icon-stt {
    background: rgba(124, 95, 214, 0.1);
    color: #7c5fd6;
  }

  .model-info {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 0.1rem;
    min-width: 0;
  }

  .model-name {
    font-size: 0.8125rem;
    font-weight: 500;
    color: #1a1a2e;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .model-meta {
    font-size: 0.6875rem;
    color: #9CA3AF;
  }

  .capitalize {
    text-transform: capitalize;
  }

  .badge-recommended {
    font-size: 0.6rem;
    font-weight: 700;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: #3435f5;
    background: rgba(52, 53, 245, 0.08);
    padding: 0.2rem 0.45rem;
    border-radius: 99px;
    flex-shrink: 0;
  }

  .badge-recommended-stt {
    color: #7c5fd6;
    background: rgba(124, 95, 214, 0.1);
  }

  /* Success row (after LLM configured) */
  .success-row {
    display: flex;
    align-items: center;
    gap: 0.625rem;
  }

  .success-icon-sm {
    width: 1.5rem;
    height: 1.5rem;
    border-radius: 50%;
    background: linear-gradient(135deg, #22c55e, #16a34a);
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
  }

  .success-filename {
    font-size: 0.8125rem;
    font-weight: 500;
    color: #16a34a;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
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
    width: 2.25rem;
    height: 1.25rem;
    border-radius: 99px;
    background: #D1D5DB;
    position: relative;
    transition: background 150ms ease;
  }

  .toggle-track::after {
    content: "";
    position: absolute;
    top: 0.1875rem;
    left: 0.1875rem;
    width: 0.875rem;
    height: 0.875rem;
    border-radius: 50%;
    background: white;
    transition: transform 150ms ease;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.15);
  }

  .toggle-track.on {
    background: #7c5fd6;
  }

  .toggle-track.on::after {
    transform: translateX(1rem);
  }

  /* Empty hints */
  .empty-hint {
    font-size: 0.8125rem;
    color: #6B7280;
    margin: 0;
    line-height: 1.55;
  }

  .empty-hint code {
    font-family: monospace;
    font-size: 0.8em;
    background: rgba(107, 114, 128, 0.1);
    padding: 0.1em 0.3em;
    border-radius: 0.25rem;
    color: #4B5563;
  }

  .inline-link {
    background: none;
    border: none;
    padding: 0;
    color: #3435f5;
    cursor: pointer;
    font-size: inherit;
    text-decoration: underline;
    text-underline-offset: 2px;
  }

  /* Errors */
  .inline-error {
    display: flex;
    align-items: center;
    gap: 0.375rem;
    font-size: 0.8125rem;
    color: #DC2626;
    background: rgba(220, 38, 38, 0.05);
    border: 1px solid rgba(220, 38, 38, 0.15);
    border-radius: 0.5rem;
    padding: 0.5rem 0.75rem;
    margin: 0;
  }

  /* Footer */
  .ai-setup-footer {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.5rem;
    padding-top: 0.25rem;
  }

  .btn-continue {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 0.375rem;
    width: 100%;
    padding: 0.75rem 1.5rem;
    border-radius: 0.875rem;
    border: none;
    background: linear-gradient(135deg, #3435f5, #7c5fd6);
    color: white;
    font-size: 0.9375rem;
    font-weight: 600;
    cursor: pointer;
    transition: opacity 150ms ease, transform 150ms ease;
    box-shadow: 0 4px 16px -2px rgba(52, 53, 245, 0.3);
  }

  .btn-continue:hover:not(:disabled) {
    transform: translateY(-1px);
    box-shadow: 0 6px 20px -2px rgba(52, 53, 245, 0.4);
  }

  .btn-continue:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .btn-skip {
    background: none;
    border: none;
    color: #9CA3AF;
    font-size: 0.8125rem;
    cursor: pointer;
    padding: 0.25rem 0.5rem;
    transition: color 150ms ease;
  }

  .btn-skip:hover:not(:disabled) {
    color: #6B7280;
  }

  .btn-skip:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
</style>
