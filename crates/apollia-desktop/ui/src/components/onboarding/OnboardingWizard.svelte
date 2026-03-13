<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import StepEnvironment from "./StepEnvironment.svelte";
  import StepFirstAgent from "./StepFirstAgent.svelte";
  import StepFirstTask from "./StepFirstTask.svelte";

  interface Props {
    onComplete: () => void;
  }

  let { onComplete }: Props = $props();

  let currentStep = $state<1 | 2 | 3>(1);
  let agentId = $state<string | null>(null);

  const steps = [
    { number: 1, label: "Environment" },
    { number: 2, label: "First agent" },
    { number: 3, label: "First task" },
  ] as const;

  async function skip() {
    try {
      await invoke("mark_onboarded");
    } catch {
      // Best effort — skip regardless
    }
    onComplete();
  }

  async function finish() {
    try {
      await invoke("mark_onboarded");
    } catch {
      // Best effort — finish regardless
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
      <h1 class="text-3xl font-bold text-primary">Welcome to Apollia OS</h1>
      <p class="mt-2 text-sm text-muted-foreground">
        Let's make sure everything is set up correctly.
      </p>
    </div>

    <!-- Stepper -->
    <div class="mb-10 flex items-center justify-center gap-8">
      {#each steps as step}
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
            {step.label}
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
        Skip setup
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
