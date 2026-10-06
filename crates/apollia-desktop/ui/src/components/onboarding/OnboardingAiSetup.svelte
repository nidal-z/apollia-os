<script lang="ts">
  /**
   * First-launch step 2 - the language model.
   *
   * The shell: the machine probe, the language-engine section and the
   * navigation. The section lives in `OnboardingLlmSection.svelte` and the
   * decisions in `aiSetupRules.ts`.
   *
   * This is the only step that cannot be passed, so it carries no "skip":
   * "Continue" opens once a model is wired, a server is registered, or a
   * download was launched. Nothing else is asked here. The context window
   * follows the default of the model, and dictation is set up in the settings.
   *
   * Designed to render inline inside the {@link OnboardingModal} overlay
   * (no own backdrop or fixed positioning).
   */
  import { t } from "svelte-i18n";
  import { getAiSetupInfo, type SystemInfo } from "$lib/ipc/models";
  import { MemoryStick, MonitorCog } from "lucide-svelte";
  import { Spinner } from "$lib/components/ui/progress";
  import { Button } from "$lib/components/ui/button";
  import { llmBackends } from "$lib/stores/sse";
  import { navigateTo } from "$lib/stores/navigation";
  import OnboardingLlmSection from "./OnboardingLlmSection.svelte";
  import { canContinueAiStep } from "./aiSetupRules";
  import { isInFlight } from "./llmDownload";
  import { llmDownload } from "./llmDownloadLive";
  import { ramLabel, osLabel } from "./onboardingFormat";
  import "./onboarding-ai-setup.css";

  interface Props {
    /**
     * Advances, with the size of the language model wired or being downloaded
     * here when there is one.
     */
    onnext: (languageModelBytes: number | null) => void;
    onback: () => void;
    /** Called when the user wants to add a cloud backend; modal should close. */
    onopencloud: () => void;
  }

  const { onnext, onback, onopencloud }: Props = $props();

  let loading = $state(true);
  let sysInfo = $state<SystemInfo | null>(null);

  /** Set by the language-engine section when it wires an engine this session. */
  let llmSuccess = $state(false);
  /** Size of the file of that engine, `null` for a server or a backend already there. */
  let llmSizeBytes = $state<number | null>(null);

  const downloading = $derived(isInFlight($llmDownload));
  const canContinue = $derived(
    canContinueAiStep({
      probing: loading,
      engineWired: llmSuccess,
      backendCount: $llmBackends.length,
      downloadInFlight: downloading,
    }),
  );

  $effect(() => {
    void probeMachine();
  });

  async function probeMachine(): Promise<void> {
    try {
      sysInfo = await getAiSetupInfo();
    } catch {
      /* the section renders without the chips */
    } finally {
      loading = false;
    }
  }

  function openCloudSettings(): void {
    navigateTo("llm");
    onopencloud();
  }

  function handleContinue(): void {
    // The next step sizes its own recommendation beside this model, wired or
    // still on its way.
    onnext(llmSizeBytes ?? (downloading ? $llmDownload.sizeBytes : null));
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
        <!-- i18n-ignore: memory unit acronym, identical in every locale -->
        <MemoryStick size={11} strokeWidth={2} />{ramLabel(sysInfo.total_ram_gb)} RAM
      </span>
      <span class="sys-chip">
        <MonitorCog size={11} strokeWidth={2} />{osLabel(sysInfo.os)} · {sysInfo.arch}
      </span>
      {#if sysInfo.gpu_available}
        <!-- i18n-ignore: accelerator acronym and memory unit, identical in every locale -->
        <span class="sys-chip sys-chip-gpu">{sysInfo.gpu_vram_gb === null ? "GPU" : `GPU · ${ramLabel(sysInfo.gpu_vram_gb)} VRAM`}</span>
      {/if}
    </div>
  {/if}

  {#if loading}
    <div class="scan-loading" data-testid="scan-loading">
      <Spinner size={18} />
      <span>{$t("onboarding.ai_setup.scanning")}</span>
    </div>
  {:else}
    <OnboardingLlmSection
      {sysInfo}
      onconfigured={(configured, sizeBytes) => {
        llmSuccess = configured;
        llmSizeBytes = sizeBytes;
      }}
      onopencloud={openCloudSettings}
    />
  {/if}

  <p class="empty-hint" data-testid="ai-setup-continue-hint">
    {downloading
      ? $t("onboarding.ai_setup.continue_while_downloading")
      : $t("onboarding.ai_setup.continue_hint")}
  </p>

  <footer class="setup-footer">
    <Button variant="ghost" size="sm" class="btn-secondary" onclick={onback} data-testid="ai-setup-back">
      ← {$t("common.back")}
    </Button>
    <div class="footer-right">
      <Button
        variant="primary-gradient"
        size="default"
        onclick={handleContinue}
        disabled={!canContinue}
        data-testid="ai-setup-continue"
      >
        {$t("common.continue")}
      </Button>
    </div>
  </footer>
</div>
