<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { open } from "@tauri-apps/plugin-dialog";
  import { Sparkles, HardDrive, Cloud, ChevronRight, Loader2, Check, AlertCircle } from "lucide-svelte";

  interface Props {
    /** Called when the LLM is configured (or skipped). */
    oncomplete: () => void;
    /** Called when the user wants to skip entirely (no LLM, no onboarding). */
    onskip: () => void;
  }

  let { oncomplete, onskip }: Props = $props();

  let selecting = $state(false);
  let configuring = $state(false);
  let error = $state<string | null>(null);
  let success = $state(false);
  let fadeIn = $state(false);

  // Trigger fade-in on mount
  $effect(() => { requestAnimationFrame(() => { fadeIn = true; }); });

  /** Maps raw backend error strings to user-friendly messages in French. */
  function toUserFriendlyError(raw: string): string {
    // Unsupported GGUF architecture (e.g. qwen35moe)
    if (raw.includes("unsupported model architecture") || raw.includes("Unknown GGUF architecture")) {
      return "Ce modèle utilise une architecture non supportée. Essayez un modèle Llama, Mistral, Qwen2 ou Phi au format GGUF.";
    }
    // Model file not found
    if (raw.includes("model file not found") || raw.includes("file not found")) {
      return "Fichier modèle introuvable. Vérifiez que le fichier .gguf existe bien à l'emplacement indiqué.";
    }
    // Device/GPU not available
    if (raw.includes("not available") && raw.includes("device")) {
      return "L'accélérateur GPU demandé n'est pas disponible sur cette machine.";
    }
    return raw;
  }

  async function handleSelectModel(): Promise<void> {
    if (selecting || configuring) return;
    selecting = true;
    error = null;

    try {
      const selected = await open({
        multiple: false,
        filters: [{ name: "GGUF Models", extensions: ["gguf"] }],
        title: "Sélectionner un modèle GGUF",
      });

      if (!selected) {
        selecting = false;
        return;
      }

      const filePath = typeof selected === "string" ? selected : selected.path;

      configuring = true;
      selecting = false;

      await invoke("setup_local_llm", { ggufPath: filePath });

      // Hot-reload the LLM router so the model is available immediately
      // without restarting the application.
      await invoke("reload_llm");

      success = true;
      // Brief pause to show the success state before transitioning
      setTimeout(() => oncomplete(), 800);
    } catch (err: unknown) {
      const raw = err instanceof Error ? err.message : String(err);
      error = toUserFriendlyError(raw);
      selecting = false;
      configuring = false;
    }
  }
</script>

<div
  class="llm-setup-fullscreen"
  class:fade-in={fadeIn}
  data-testid="onboarding-llm-setup"
>
  <header class="llm-setup-header">
    <div class="llm-setup-logo">
      <Sparkles size={28} strokeWidth={1.5} class="text-white" />
    </div>
    <h1 class="llm-setup-title">Moteur IA</h1>
    <p class="llm-setup-subtitle">
      Apollia a besoin d'un mod&egrave;le de langage pour converser avec vous.
      <br />
      S&eacute;lectionnez un fichier <strong>.gguf</strong> d&eacute;j&agrave; pr&eacute;sent sur votre machine.
    </p>
  </header>

  <main class="llm-setup-content">
    {#if success}
      <div class="llm-setup-success" data-testid="llm-setup-success">
        <div class="success-icon">
          <Check size={28} strokeWidth={2.5} class="text-white" />
        </div>
        <p class="success-text">Mod&egrave;le configur&eacute; avec succ&egrave;s</p>
      </div>
    {:else}
      <button
        class="llm-setup-option llm-option-primary"
        data-testid="llm-select-model"
        onclick={handleSelectModel}
        disabled={selecting || configuring}
      >
        <div class="option-icon-wrap option-icon-local">
          <HardDrive size={20} strokeWidth={1.5} />
        </div>
        <div class="option-text">
          <span class="option-title">S&eacute;lectionner mon mod&egrave;le</span>
          <span class="option-desc">Fichier .gguf local (Llama, Mistral, Qwen, Phi&hellip;)</span>
        </div>
        <div class="option-action">
          {#if configuring}
            <Loader2 size={16} class="animate-spin text-white/70" />
          {:else}
            <ChevronRight size={16} class="text-white/50" />
          {/if}
        </div>
      </button>

      <div class="llm-setup-divider">
        <span>ou</span>
      </div>

      <button
        class="llm-setup-option llm-option-secondary"
        data-testid="llm-configure-cloud"
        onclick={oncomplete}
        disabled={selecting || configuring}
      >
        <div class="option-icon-wrap option-icon-cloud">
          <Cloud size={20} strokeWidth={1.5} />
        </div>
        <div class="option-text">
          <span class="option-title">Je configurerai plus tard</span>
          <span class="option-desc">Provider cloud (Anthropic, OpenAI&hellip;) ou mod&egrave;le local</span>
        </div>
        <ChevronRight size={16} class="text-gray-400" />
      </button>

      {#if error}
        <div class="llm-setup-error" data-testid="llm-setup-error">
          <AlertCircle size={14} />
          <span>{error}</span>
        </div>
      {/if}
    {/if}
  </main>

  <footer class="llm-setup-footer">
    <p class="llm-setup-hint">
      Pas de mod&egrave;le ? T&eacute;l&eacute;chargez-en un sur
      <a href="https://huggingface.co/models?library=gguf" target="_blank" rel="noopener noreferrer">
        HuggingFace
      </a>
      (recommand&eacute; : &ge; 0.5B param&egrave;tres, format Q4_K_M ou Q8_0).
    </p>
    <button
      class="btn-skip"
      data-testid="llm-setup-skip"
      onclick={onskip}
    >
      Passer pour l'instant
    </button>
  </footer>
</div>

<style>
  .llm-setup-fullscreen {
    position: fixed;
    inset: 0;
    z-index: 50;
    background: #FFF8F0;
    display: flex;
    flex-direction: column;
    align-items: center;
    opacity: 0;
    transition: opacity 300ms ease-in;
  }

  .llm-setup-fullscreen.fade-in {
    opacity: 1;
  }

  .llm-setup-header {
    display: flex;
    flex-direction: column;
    align-items: center;
    padding: 3rem 1rem 1.5rem;
    gap: 0.5rem;
  }

  .llm-setup-logo {
    width: 3.5rem;
    height: 3.5rem;
    border-radius: 1rem;
    background: linear-gradient(135deg, #3435f5, #7c5fd6);
    display: flex;
    align-items: center;
    justify-content: center;
    box-shadow:
      0 4px 16px -2px rgba(52, 53, 245, 0.25),
      0 8px 32px -8px rgba(124, 95, 214, 0.2);
  }

  .llm-setup-title {
    font-size: 1.5rem;
    font-weight: 700;
    color: #1a1a2e;
    margin: 0;
  }

  .llm-setup-subtitle {
    font-size: 0.875rem;
    color: #6B7280;
    margin: 0;
    text-align: center;
    line-height: 1.5;
    max-width: 28rem;
  }

  .llm-setup-subtitle strong {
    color: #4B5563;
  }

  .llm-setup-content {
    flex: 1;
    width: 100%;
    max-width: 28rem;
    padding: 1rem 1rem;
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }

  .llm-setup-option {
    display: flex;
    align-items: center;
    gap: 0.875rem;
    padding: 1rem 1.25rem;
    border-radius: 1rem;
    border: none;
    cursor: pointer;
    text-align: left;
    transition: transform 150ms ease, box-shadow 150ms ease;
    width: 100%;
  }

  .llm-setup-option:hover:not(:disabled) {
    transform: translateY(-1px);
  }

  .llm-setup-option:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .llm-option-primary {
    background: linear-gradient(135deg, #3435f5, #7c5fd6);
    color: white;
    box-shadow:
      0 4px 16px -2px rgba(52, 53, 245, 0.3),
      0 8px 32px -8px rgba(124, 95, 214, 0.2);
  }

  .llm-option-primary:hover:not(:disabled) {
    box-shadow:
      0 6px 20px -2px rgba(52, 53, 245, 0.4),
      0 12px 40px -8px rgba(124, 95, 214, 0.3);
  }

  .llm-option-secondary {
    background: rgba(255, 255, 255, 0.72);
    backdrop-filter: blur(20px);
    border: 1px solid rgba(52, 53, 245, 0.08);
    color: #374151;
    box-shadow:
      inset 0 0 0 1px rgba(255, 255, 255, 0.5),
      0 1px 2px rgba(0, 0, 0, 0.03);
  }

  .llm-option-secondary:hover:not(:disabled) {
    border-color: rgba(52, 53, 245, 0.15);
    box-shadow:
      inset 0 0 0 1px rgba(255, 255, 255, 0.5),
      0 4px 12px rgba(0, 0, 0, 0.06);
  }

  .option-icon-wrap {
    width: 2.5rem;
    height: 2.5rem;
    border-radius: 0.75rem;
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
  }

  .option-icon-local {
    background: rgba(255, 255, 255, 0.2);
  }

  .option-icon-cloud {
    background: rgba(52, 53, 245, 0.08);
    color: #7c5fd6;
  }

  .option-text {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 0.125rem;
  }

  .option-title {
    font-size: 0.9375rem;
    font-weight: 600;
  }

  .option-desc {
    font-size: 0.75rem;
    opacity: 0.7;
  }

  .option-action {
    flex-shrink: 0;
  }

  .llm-setup-divider {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    color: #9CA3AF;
    font-size: 0.75rem;
    padding: 0.25rem 0;
  }

  .llm-setup-divider::before,
  .llm-setup-divider::after {
    content: "";
    flex: 1;
    height: 1px;
    background: #E5E7EB;
  }

  .llm-setup-success {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 1rem;
    padding: 3rem 0;
  }

  .success-icon {
    width: 3.5rem;
    height: 3.5rem;
    border-radius: 50%;
    background: linear-gradient(135deg, #22c55e, #16a34a);
    display: flex;
    align-items: center;
    justify-content: center;
    box-shadow: 0 4px 16px rgba(34, 197, 94, 0.3);
    animation: pop-in 300ms ease;
  }

  .success-text {
    font-size: 1rem;
    font-weight: 600;
    color: #16a34a;
    margin: 0;
  }

  .llm-setup-error {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.625rem 1rem;
    border-radius: 0.75rem;
    background: #FEF2F2;
    color: #DC2626;
    font-size: 0.8125rem;
  }

  .llm-setup-footer {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.5rem;
    padding: 0.5rem 1rem 1.5rem;
  }

  .llm-setup-hint {
    font-size: 0.75rem;
    color: #9CA3AF;
    text-align: center;
    margin: 0;
  }

  .llm-setup-hint a {
    color: #7c5fd6;
    text-decoration: none;
  }

  .llm-setup-hint a:hover {
    text-decoration: underline;
  }

  .btn-skip {
    background: none;
    border: none;
    color: #6B7280;
    font-size: 0.8125rem;
    cursor: pointer;
    padding: 0.25rem 0.5rem;
    transition: color 150ms ease;
  }

  .btn-skip:hover {
    text-decoration: underline;
    color: #4B5563;
  }

  @keyframes pop-in {
    0% { transform: scale(0.5); opacity: 0; }
    70% { transform: scale(1.1); }
    100% { transform: scale(1); opacity: 1; }
  }
</style>
