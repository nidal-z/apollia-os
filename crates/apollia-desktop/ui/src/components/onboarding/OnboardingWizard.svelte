<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import { uiMode } from "$lib/stores/mode";
  import type { UIMode } from "$lib/stores/mode";
  import StepWelcome from "./StepWelcome.svelte";
  import StepEnvironment from "./StepEnvironment.svelte";
  import StepFirstAgent from "./StepFirstAgent.svelte";
  import StepFirstTask from "./StepFirstTask.svelte";
  import StepChooseAssistant from "./StepChooseAssistant.svelte";
  import StepSeeItWork from "./StepSeeItWork.svelte";
  import StepExplore from "./StepExplore.svelte";

  interface Props {
    onComplete: () => void;
  }

  let { onComplete }: Props = $props();

  /** Wizard state: welcome screen, then path-specific steps. */
  type WizardStep = "welcome" | "environment" | "agent" | "task" | "explore";

  let currentStep = $state<WizardStep>("welcome");
  let selectedMode = $state<UIMode | null>(null);
  let agentId = $state<string | null>(null);
  let agentName = $state<string | null>(null);

  let isOperator = $derived(selectedMode === "operator");
  let isBuilder = $derived(selectedMode === "builder");

  /** Step definitions per path, for the stepper UI. */
  const OPERATOR_STEPS: { step: WizardStep; labelKey: string }[] = [
    { step: "environment", labelKey: "onboarding.step_environment" },
    { step: "agent", labelKey: "onboarding.step_choose_assistant" },
    { step: "task", labelKey: "onboarding.step_see_it_work" },
  ];

  const BUILDER_STEPS: { step: WizardStep; labelKey: string }[] = [
    { step: "environment", labelKey: "onboarding.step_environment" },
    { step: "agent", labelKey: "onboarding.step_first_agent" },
    { step: "task", labelKey: "onboarding.step_first_task" },
    { step: "explore", labelKey: "onboarding.step_explore" },
  ];

  let steps = $derived(isOperator ? OPERATOR_STEPS : BUILDER_STEPS);

  let currentStepIndex = $derived(
    steps.findIndex((s) => s.step === currentStep)
  );

  function handleProfileChoice(profile: UIMode) {
    selectedMode = profile;
    uiMode.set(profile);
    currentStep = "environment";
  }

  function handleAgentStarted(id: string, name: string) {
    agentId = id;
    agentName = name;
    currentStep = "task";
  }

  async function skip() {
    try {
      await invoke("mark_onboarded");
    } catch {
      // Best effort
    }
    onComplete();
  }

  async function finish() {
    try {
      await invoke("mark_onboarded");
    } catch {
      // Best effort
    }
    onComplete();
  }
</script>

<div class="fixed inset-0 z-50 flex items-center justify-center bg-background">
  <div class="flex w-full max-w-xl flex-col px-6 py-10">
    {#if currentStep === "welcome"}
      <StepWelcome onChoose={handleProfileChoice} />
    {:else}
      <!-- Stepper -->
      <div class="mb-10 flex items-center justify-center gap-6">
        {#each steps as step, i}
          <div class="flex items-center gap-2">
            <span
              class="flex h-8 w-8 items-center justify-center rounded-full text-sm font-semibold
                {currentStepIndex === i
                  ? 'bg-primary text-primary-foreground'
                  : currentStepIndex > i
                    ? 'bg-primary/20 text-primary'
                    : 'bg-muted text-muted-foreground'}"
            >
              {#if currentStepIndex > i}
                &#10003;
              {:else}
                {i + 1}
              {/if}
            </span>
            <span
              class="text-sm {currentStepIndex === i
                ? 'font-medium'
                : 'text-muted-foreground'}"
            >
              {$t(step.labelKey)}
            </span>
          </div>
        {/each}
      </div>

      <!-- Skip button -->
      <div class="absolute right-6 top-6">
        <button
          class="text-sm text-muted-foreground underline-offset-4 hover:underline"
          onclick={skip}
        >
          {$t('onboarding.skip_setup')}
        </button>
      </div>

      <!-- Step content -->
      <div class="flex-1">
        {#if currentStep === "environment" && selectedMode}
          <StepEnvironment
            mode={selectedMode}
            onContinue={() => (currentStep = "agent")}
            onSkip={skip}
          />
        {:else if currentStep === "agent" && isOperator}
          <StepChooseAssistant
            onAgentStarted={handleAgentStarted}
            onSkip={skip}
          />
        {:else if currentStep === "agent" && isBuilder}
          <StepFirstAgent
            onAgentStarted={handleAgentStarted}
            onSkip={skip}
          />
        {:else if currentStep === "task" && isOperator && agentId && agentName}
          <StepSeeItWork
            agentId={agentId}
            agentName={agentName}
            onFinish={finish}
          />
        {:else if currentStep === "task" && isBuilder && agentId}
          <StepFirstTask
            agentId={agentId}
            onFinish={() => (currentStep = "explore")}
          />
        {:else if currentStep === "explore" && isBuilder}
          <StepExplore onFinish={finish} />
        {/if}
      </div>
    {/if}
  </div>
</div>
