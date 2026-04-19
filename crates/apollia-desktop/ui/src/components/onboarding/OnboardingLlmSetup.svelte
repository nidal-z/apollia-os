<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { open } from "@tauri-apps/plugin-dialog";
  import { t } from "svelte-i18n";
  import { Sparkles, HardDrive, Cloud, ChevronRight, Check, AlertCircle } from "lucide-svelte";
  import { Spinner } from "$lib/components/ui/progress";

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
        title: $t("onboarding_v2.llm_setup.dialog_title"),
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
    <h1 class="llm-setup-title">{$t("onboarding_v2.llm_setup.title")}</h1>
    <p class="llm-setup-subtitle">{$t("onboarding_v2.llm_setup.subtitle")}</p>
  </header>

  <main class="llm-setup-content">
    {#if success}
      <div class="llm-setup-success" data-testid="llm-setup-success">
        <div class="success-icon">
          <Check size={28} strokeWidth={2.5} class="text-white" />
        </div>
        <p class="success-text">{$t("onboarding_v2.llm_setup.success")}</p>
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
          <span class="option-title">{$t("onboarding_v2.llm_setup.local_title")}</span>
          <span class="option-desc">{$t("onboarding_v2.llm_setup.local_desc")}</span>
        </div>
        <div class="option-action">
          {#if configuring}
            <Spinner size={16} />
          {:else}
            <ChevronRight size={16} class="text-white/50" />
          {/if}
        </div>
      </button>

      <div class="llm-setup-divider">
        <span>{$t("onboarding_v2.llm_setup.divider")}</span>
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
          <span class="option-title">{$t("onboarding_v2.llm_setup.later_title")}</span>
          <span class="option-desc">{$t("onboarding_v2.llm_setup.later_desc")}</span>
        </div>
        <ChevronRight size={16} class="text-muted-foreground/70" />
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
      {$t("onboarding_v2.llm_setup.hint")}
    </p>
    <button
      class="btn-skip"
      data-testid="llm-setup-skip"
      onclick={onskip}
    >
      {$t("onboarding_v2.llm_setup.skip")}
    </button>
  </footer>
</div>

<style>
  .llm-setup-fullscreen {
    position: fixed;
    inset: 0;
    z-index: 50;
    background: hsl(var(--background));
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
    background: var(--gradient-primary);
    display: flex;
    align-items: center;
    justify-content: center;
    box-shadow: var(--shadow-primary-md), var(--shadow-primary-xl);
  }

  .llm-setup-title {
    font-size: 1.5rem;
    font-weight: 700;
    color: hsl(var(--foreground));
    margin: 0;
  }

  .llm-setup-subtitle {
    font-size: 0.875rem;
    color: hsl(var(--muted-foreground));
    margin: 0;
    text-align: center;
    line-height: 1.5;
    max-width: 28rem;
  }

  .llm-setup-subtitle strong {
    color: hsl(var(--foreground) / 0.8);
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
    background: linear-gradient(135deg, hsl(var(--primary-gradient-from)), hsl(var(--primary-gradient-to)));
    color: white;
    box-shadow: var(--shadow-primary-md), var(--shadow-primary-xl);
  }

  .llm-option-primary:hover:not(:disabled) {
    box-shadow: var(--shadow-primary-lg), var(--shadow-primary-xl);
  }

  .llm-option-secondary {
    background: hsl(var(--card) / 0.72);
    backdrop-filter: blur(20px);
    border: 1px solid hsl(var(--primary) / 0.08);
    color: hsl(var(--foreground));
    box-shadow: var(--shadow-elev-1);
  }

  .llm-option-secondary:hover:not(:disabled) {
    border-color: hsl(var(--primary) / 0.15);
    box-shadow: var(--shadow-elev-2);
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
    background: hsl(var(--card) / 0.2);
  }

  .option-icon-cloud {
    background: hsl(var(--primary) / 0.08);
    color: hsl(var(--secondary));
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
    color: hsl(var(--muted-foreground) / 0.7);
    font-size: 0.75rem;
    padding: 0.25rem 0;
  }

  .llm-setup-divider::before,
  .llm-setup-divider::after {
    content: "";
    flex: 1;
    height: 1px;
    background: hsl(var(--border));
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
    background: linear-gradient(135deg, hsl(var(--success)), hsl(var(--success) / 0.8));
    display: flex;
    align-items: center;
    justify-content: center;
    box-shadow: var(--shadow-success-md);
    animation: pop-in 300ms ease;
  }

  .success-text {
    font-size: 1rem;
    font-weight: 600;
    color: hsl(var(--success));
    margin: 0;
  }

  .llm-setup-error {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.625rem 1rem;
    border-radius: 0.75rem;
    background: hsl(var(--destructive) / 0.05);
    color: hsl(var(--destructive));
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
    color: hsl(var(--muted-foreground) / 0.7);
    text-align: center;
    margin: 0;
  }

  .llm-setup-hint a {
    color: hsl(var(--secondary));
    text-decoration: none;
  }

  .llm-setup-hint a:hover {
    text-decoration: underline;
  }

  .btn-skip {
    background: none;
    border: none;
    color: hsl(var(--muted-foreground));
    font-size: 0.8125rem;
    cursor: pointer;
    padding: 0.25rem 0.5rem;
    transition: color 150ms ease;
  }

  .btn-skip:hover {
    text-decoration: underline;
    color: hsl(var(--foreground) / 0.8);
  }

  @keyframes pop-in {
    0% { transform: scale(0.5); opacity: 0; }
    70% { transform: scale(1.1); }
    100% { transform: scale(1); opacity: 1; }
  }
</style>
