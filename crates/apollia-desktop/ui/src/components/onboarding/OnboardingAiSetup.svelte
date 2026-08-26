<script lang="ts">
  /**
   * Onboarding step 3 - AI Setup.
   *
   * The shell: the machine probe, the two sections, and the navigation. The
   * language-engine half lives in `OnboardingLlmSection.svelte`, the voice half
   * in `OnboardingSttSection.svelte`, and the decisions both make in
   * `aiSetupRules.ts`.
   *
   * Designed to render inline inside the {@link OnboardingModal} overlay
   * (no own backdrop or fixed positioning).
   */
  import { get } from "svelte/store";
  import { t, locale } from "svelte-i18n";
  import {
    getAiSetupInfo,
    setupWhisperModel,
    type SystemInfo,
    type WhisperModelInfo,
  } from "$lib/ipc/models";
  import { AlertCircle, MemoryStick, MonitorCog } from "lucide-svelte";
  import { Spinner } from "$lib/components/ui/progress";
  import { Button } from "$lib/components/ui/button";
  import { llmBackends } from "$lib/stores/sse";
  import { navigateTo } from "$lib/stores/navigation";
  import OnboardingLlmSection from "./OnboardingLlmSection.svelte";
  import OnboardingSttSection from "./OnboardingSttSection.svelte";
  import { ramLabel, osLabel } from "./onboardingFormat";
  import "./onboarding-ai-setup.css";

  interface Props {
    onnext: () => void;
    onback: () => void;
    onskip: () => void;
    /** Called when the user wants to add a cloud backend; modal should close. */
    onopencloud: () => void;
  }

  const { onnext, onback, onskip, onopencloud }: Props = $props();

  /** Base app locale (e.g. "fr", "en") to seed the STT transcription language,
   * so dictation transcribes in the user's language instead of English. */
  function currentLocale(): string | undefined {
    const l = get(locale);
    return l ? l.split("-")[0] : undefined;
  }

  let loading = $state(true);
  let sysInfo = $state<SystemInfo | null>(null);

  /** Set by the language-engine section when it wires an engine this session. */
  let llmSuccess = $state(false);
  /** Mirrors the voice section, which the step persists before it advances. */
  let voice = $state<{ enabled: boolean; model: WhisperModelInfo | null }>({
    enabled: false,
    model: null,
  });

  let advancing = $state(false);
  let advanceError = $state<string | null>(null);

  /**
   * The continue button should only enable once the runtime can actually
   * answer the agent's first turn - i.e. either the local LLM was wired up
   * during this session, or a backend (cloud or pre-existing local) is
   * already registered in the runtime.
   */
  const hasUsableLlm = $derived(llmSuccess || $llmBackends.length > 0);

  $effect(() => {
    void probeMachine();
  });

  async function probeMachine(): Promise<void> {
    try {
      sysInfo = await getAiSetupInfo();
    } catch {
      /* the sections render without the chips */
    } finally {
      loading = false;
    }
  }

  function openCloudSettings(): void {
    navigateTo("llm");
    onopencloud();
  }

  async function handleContinue(): Promise<void> {
    if (advancing) return;
    advancing = true;
    advanceError = null;
    try {
      if (voice.enabled && voice.model) {
        await setupWhisperModel(voice.model.path, currentLocale());
      }
      onnext();
    } catch (err: unknown) {
      advanceError = err instanceof Error ? err.message : String(err);
      advancing = false;
    }
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
      {#if sysInfo.gpu_available}<span class="sys-chip sys-chip-gpu">GPU</span>{/if}
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
      onconfigured={(configured) => (llmSuccess = configured)}
      onopencloud={openCloudSettings}
    />

    <OnboardingSttSection
      {sysInfo}
      locale={currentLocale}
      onchange={(choice) => (voice = choice)}
    />
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
