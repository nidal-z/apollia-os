<script lang="ts">
  import { t } from 'svelte-i18n';
  import { Check } from 'lucide-svelte';
  import type { TourStep } from '$lib/types';

  interface Props {
    /** Full list of tour steps. */
    steps: TourStep[];
    /** Zero-based flat index of the current step. */
    currentStep: number;
  }

  let { steps, currentStep }: Props = $props();

  interface GroupInfo {
    label: string;
    startIndex: number;
    endIndex: number;
    count: number;
  }

  let groups = $derived.by(() => {
    const result: GroupInfo[] = [];
    for (let i = 0; i < steps.length; i++) {
      const g = steps[i].group ?? steps[i].id;
      const last = result[result.length - 1];
      if (last && last.label === g) {
        last.endIndex = i;
        last.count++;
      } else {
        result.push({ label: g, startIndex: i, endIndex: i, count: 1 });
      }
    }
    return result;
  });

  type StepState = 'completed' | 'current' | 'future';

  function groupState(group: GroupInfo): StepState {
    if (currentStep > group.endIndex) return 'completed';
    if (currentStep >= group.startIndex) return 'current';
    return 'future';
  }

  function subProgress(group: GroupInfo): string {
    if (group.count <= 1) return '';
    const sub = Math.min(currentStep - group.startIndex + 1, group.count);
    return `${sub}/${group.count}`;
  }
</script>

<nav
  class="progress-rail"
  aria-label={$t("onboarding_v2.tour.progress_label")}
  data-testid="tour-progress-rail"
>
  <ol class="steps-list">
    {#each groups as group, gi (group.label)}
      {@const state = groupState(group)}
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

        {#if state === 'current'}
          <div class="step-info">
            <span class="step-label">{$t(`onboarding_v2.tour.group.${group.label}`)}</span>
            {#if group.count > 1}
              <span class="step-sub">{subProgress(group)}</span>
            {/if}
          </div>
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
    box-shadow: var(--shadow-elev-2);
  }

  .step-item {
    position: relative;
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  /* Connector line between groups */
  .step-item:not(:last-child)::after {
    content: '';
    position: absolute;
    left: 9px;
    top: calc(50% + 8px);
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

  .step-item.completed .step-dot {
    background: #22c55e;
    color: #ffffff;
    border: none;
  }

  .step-item.current .step-dot {
    --rail-dot-rest: 0 0 0 3px hsl(var(--primary) / 0.25);
    --rail-dot-pulse: 0 0 0 4px hsl(var(--primary) / 0.15);
    background: hsl(var(--primary));
    border: 2.5px solid hsl(var(--primary));
    box-shadow: var(--rail-dot-rest);
    animation: rail-pulse 2s ease-in-out infinite;
  }

  .step-item.future .step-dot {
    background: transparent;
    border: 1.5px dashed #d1d5db;
  }

  @keyframes rail-pulse {
    0%, 100% {
      transform: scale(1);
      box-shadow: var(--rail-dot-rest);
    }
    50% {
      transform: scale(1.2);
      box-shadow: var(--rail-dot-pulse);
    }
  }

  .step-info {
    display: flex;
    flex-direction: column;
    gap: 1px;
  }

  .step-label {
    font-size: 0.6875rem;
    font-weight: 600;
    color: #3435f5;
    white-space: nowrap;
    line-height: 1;
  }

  .step-sub {
    font-size: 0.5625rem;
    font-weight: 500;
    color: rgba(52, 53, 245, 0.6);
    white-space: nowrap;
    line-height: 1;
  }
</style>
