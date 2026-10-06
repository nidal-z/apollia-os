<script lang="ts">
  /**
   * First-launch step 2 - the language model.
   *
   * The shell: the machine probe, the choice between a model on this computer
   * and a server, and the navigation. The first lives in
   * `OnboardingLlmSection.svelte`, the second in `OnboardingServerForm.svelte`,
   * and the decisions in `aiSetupRules.ts` and `serverSetup.ts`.
   *
   * This is the only step that cannot be passed, so it carries no "skip":
   * "Continue" opens once a model is wired, a server model is chosen, or a
   * download was launched. Nothing else is asked here, and nothing here
   * leaves the first launch: the server is declared inside the step. The context window
   * follows the default of the model, and dictation is set up in the settings.
   *
   * Designed to render inline inside the {@link OnboardingModal} overlay
   * (no own backdrop or fixed positioning).
   */
  import { t } from "svelte-i18n";
  import { getAiSetupInfo, type SystemInfo } from "$lib/ipc/models";
  import { Laptop, MemoryStick, MonitorCog, Server } from "lucide-svelte";
  import { Spinner } from "$lib/components/ui/progress";
  import { Button } from "$lib/components/ui/button";
  import { llmBackends } from "$lib/stores/sse";
  import OnboardingLlmSection from "./OnboardingLlmSection.svelte";
  import OnboardingServerForm from "./OnboardingServerForm.svelte";
  import { canContinueAiStep, type AiSource } from "./aiSetupRules";
  import { isInFlight } from "./llmDownload";
  import { llmDownload } from "./llmDownloadLive";
  import { ramLabel, osLabel } from "./onboardingFormat";
  import { serverSetup } from "./serverSetupLive";
  import "./onboarding-ai-setup.css";

  interface Props {
    /**
     * Advances, with the size of the language model wired or being downloaded
     * here when there is one.
     */
    onnext: (languageModelBytes: number | null) => void;
    onback: () => void;
  }

  const { onnext, onback }: Props = $props();

  const SOURCES: { id: AiSource; icon: typeof Laptop; title: string; body: string }[] = [
    {
      id: "local",
      icon: Laptop,
      title: "onboarding.ai_setup.source.local_title",
      body: "onboarding.ai_setup.source.local_body",
    },
    {
      id: "server",
      icon: Server,
      title: "onboarding.ai_setup.source.server_title",
      body: "onboarding.ai_setup.source.server_body",
    },
  ];

  let loading = $state(true);
  let sysInfo = $state<SystemInfo | null>(null);

  /** Set by the language-engine section when it wires an engine this session. */
  let llmSuccess = $state(false);
  /** Size of the file of that engine, `null` for a server or a backend already there. */
  let llmSizeBytes = $state<number | null>(null);

  /** Which of the two the step shows. A server already chosen is shown again. */
  let source = $state<AiSource>($serverSetup.chosen === null ? "local" : "server");

  const downloading = $derived(isInFlight($llmDownload));
  const canContinue = $derived(
    canContinueAiStep({
      probing: loading,
      engineWired: llmSuccess,
      backendCount: $llmBackends.length,
      serverChosen: $serverSetup.chosen !== null,
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
    <div
      class="source-cards"
      role="radiogroup"
      aria-label={$t("onboarding.ai_setup.source.label")}
      data-testid="ai-setup-source"
    >
      {#each SOURCES as { id, icon: SourceIcon, title, body } (id)}
        <button
          type="button"
          role="radio"
          aria-checked={source === id}
          class="source-card"
          class:source-card-chosen={source === id}
          onclick={() => (source = id)}
          data-testid={`ai-setup-source-${id}`}
        >
          <SourceIcon size={16} strokeWidth={2} aria-hidden="true" />
          <span class="source-card-text">
            <span class="source-card-title">{$t(title)}</span>
            <span class="source-card-body">{$t(body)}</span>
          </span>
        </button>
      {/each}
    </div>

    {#if source === "local"}
      <OnboardingLlmSection
        {sysInfo}
        onconfigured={(configured, sizeBytes) => {
          llmSuccess = configured;
          llmSizeBytes = sizeBytes;
        }}
      />
    {:else}
      <OnboardingServerForm setup={serverSetup} />
    {/if}
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
