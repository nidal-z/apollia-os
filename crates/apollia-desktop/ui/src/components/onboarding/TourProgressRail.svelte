<script lang="ts">
  import { Check } from 'lucide-svelte';

  interface Props {
    /** Total number of steps in the tour. */
    totalSteps: number;
    /** Zero-based index of the current step. */
    currentStep: number;
    /** Optional label for each step (indexed by step position). */
    stepLabels?: string[];
  }

  let { totalSteps, currentStep, stepLabels = [] }: Props = $props();

  type StepState = 'completed' | 'current' | 'future';

  function stepState(index: number): StepState {
    if (index < currentStep) return 'completed';
    if (index === currentStep) return 'current';
    return 'future';
  }
</script>

<nav
  class="progress-rail"
  aria-label="Progression du tour"
  data-testid="tour-progress-rail"
>
  <ol class="steps-list">
    {#each Array.from({ length: totalSteps }, (_, i) => i) as index (index)}
      {@const state = stepState(index)}
      <li
        class="step-item"
        class:completed={state === 'completed'}
        class:current={state === 'current'}
        class:future={state === 'future'}
        aria-current={state === 'current' ? 'step' : undefined}
      >
        <div class="step-dot" aria-hidden="true">
          {#if state === 'completed'}
            <Check size={10} strokeWidth={3} />
          {/if}
        </div>

        {#if stepLabels[index]}
          <span class="step-label">{stepLabels[index]}</span>
        {/if}
      </li>
    {/each}
  </ol>
</nav>

<style>
  .progress-rail {
    position: fixed;
    left: 1rem;
    top: 50%;
    transform: translateY(-50%);
    z-index: 62;
  }

  .steps-list {
    list-style: none;
    margin: 0;
    padding: 0.75rem 0.625rem;
    display: flex;
    flex-direction: column;
    gap: 0;
    background: rgba(255, 255, 255, 0.88);
    border-radius: 2rem;
    box-shadow:
      0 0 0 1px rgba(52, 53, 245, 0.08),
      0 4px 16px -4px rgba(0, 0, 0, 0.1);
  }

  .step-item {
    position: relative;
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  /* Connector line between steps */
  .step-item:not(:last-child)::after {
    content: '';
    position: absolute;
    left: 50%;
    top: calc(50% + 8px);
    transform: translateX(-50%);
    width: 1.5px;
    height: 12px;
    background: rgba(0, 0, 0, 0.1);
  }

  .step-item.completed:not(:last-child)::after {
    background: #22c55e;
  }

  .step-dot {
    width: 18px;
    height: 18px;
    border-radius: 50%;
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    margin: 7px 0;
  }

  /* Completed step: solid green */
  .step-item.completed .step-dot {
    background: #22c55e;
    color: #ffffff;
    border: none;
  }

  /* Current step: blue with pulse */
  .step-item.current .step-dot {
    background: #3435f5;
    border: 2.5px solid #3435f5;
    box-shadow: 0 0 0 3px rgba(52, 53, 245, 0.25);
    animation: rail-pulse 2s ease-in-out infinite;
  }

  /* Future step: gray with dashed border */
  .step-item.future .step-dot {
    background: transparent;
    border: 1.5px dashed #d1d5db;
  }

  @keyframes rail-pulse {
    0%, 100% {
      transform: scale(1);
      box-shadow: 0 0 0 3px rgba(52, 53, 245, 0.25);
    }
    50% {
      transform: scale(1.2);
      box-shadow: 0 0 0 4px rgba(52, 53, 245, 0.15);
    }
  }

  .step-label {
    font-size: 0.75rem;
    font-weight: 500;
    color: #6b7280;
    white-space: nowrap;
    line-height: 1;
  }

  .step-item.completed .step-label {
    color: #22c55e;
  }

  .step-item.current .step-label {
    color: #3435f5;
    font-weight: 600;
  }
</style>
