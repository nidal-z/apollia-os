<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import StepEnvironment from "./StepEnvironment.svelte";
  import StepFirstAgent from "./StepFirstAgent.svelte";
  import StepFirstTask from "./StepFirstTask.svelte";

  interface Props {
    onComplete: () => void;
  }

  let { onComplete }: Props = $props();

  let currentStep = $state<1 | 2 | 3>(1);
  let agentId = $state<string | null>(null);

  const STEP_KEYS = [
    { number: 1, labelKey: "onboarding.step_environment" },
    { number: 2, labelKey: "onboarding.step_first_agent" },
    { number: 3, labelKey: "onboarding.step_first_task" },
  ] as const;

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

  function handleAgentStarted(id: string) {
    agentId = id;
    currentStep = 3;
  }
</script>

<div class="fixed inset-0 z-50 flex items-center justify-center bg-background">
  <div class="flex w-full max-w-xl flex-col px-6 py-10">
    <!-- Header -->
    <div class="mb-8 text-center">
      <h1 class="text-3xl font-bold text-primary">{$t('onboarding.welcome_title')}</h1>
      <p class="mt-2 text-sm text-muted-foreground">
        {$t('onboarding.welcome_subtitle')}
      </p>
    </div>

    <!-- Stepper -->
    <div class="mb-10 flex items-center justify-center gap-8">
      {#each STEP_KEYS as step}
        <div class="flex items-center gap-2">
          <span
            class="flex h-8 w-8 items-center justify-center rounded-full text-sm font-semibold
              {currentStep === step.number
                ? 'bg-primary text-primary-foreground'
                : currentStep > step.number
                  ? 'bg-primary/20 text-primary'
                  : 'bg-muted text-muted-foreground'}"
          >
            {#if currentStep > step.number}
              &#10003;
            {:else}
              {step.number}
            {/if}
          </span>
          <span
            class="text-sm {currentStep === step.number
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
      {#if currentStep === 1}
        <StepEnvironment
          onContinue={() => (currentStep = 2)}
          onSkip={skip}
        />
      {:else if currentStep === 2}
        <StepFirstAgent
          onAgentStarted={handleAgentStarted}
          onSkip={skip}
        />
      {:else if currentStep === 3 && agentId}
        <StepFirstTask
          agentId={agentId}
          onFinish={finish}
        />
      {/if}
    </div>
  </div>
</div>
