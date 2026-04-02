<script lang="ts">
  import { t } from "svelte-i18n";

  interface Props {
    /** Index of the current onboarding phase (1-based). */
    currentStep: number;
    /** Total number of onboarding phases. */
    totalSteps: number;
    /** Called when the user clicks the resume button. */
    onresume: () => void;
    /** Called when the user dismisses the bar for this session. */
    ondismiss: () => void;
  }

  let { currentStep, totalSteps, onresume, ondismiss }: Props = $props();
</script>

<div class="resume-bar" data-testid="onboarding-resume-bar" role="banner">
  <span class="step-label">
    {$t("onboarding_v2.resume_bar.step_label", { values: { current: currentStep, total: totalSteps } })}
  </span>
  <div class="actions">
    <button class="btn-resume" onclick={onresume}>
      {$t("onboarding_v2.resume_bar.resume_btn")}
    </button>
    <button class="btn-dismiss" onclick={ondismiss}>
      {$t("onboarding_v2.resume_bar.dismiss_btn")}
    </button>
  </div>
</div>

<style>
  .resume-bar {
    height: 48px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 1.25rem;
    background: linear-gradient(90deg, rgba(52, 53, 245, 0.06) 0%, rgba(124, 95, 214, 0.06) 100%);
    border-bottom: 1px solid rgba(52, 53, 245, 0.10);
    flex-shrink: 0;
    gap: 0.75rem;
  }

  .step-label {
    font-size: 0.8125rem;
    font-weight: 500;
    color: #4B5563;
  }

  .actions {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    flex-shrink: 0;
  }

  .btn-resume {
    padding: 0.3125rem 0.875rem;
    border-radius: 0.5rem;
    background: #3435f5;
    color: white;
    font-size: 0.8125rem;
    font-weight: 600;
    border: none;
    cursor: pointer;
    transition: background 150ms ease;
    white-space: nowrap;
  }

  .btn-resume:hover {
    background: #2a2bd4;
  }

  .btn-dismiss {
    padding: 0.3125rem 0.75rem;
    border-radius: 0.5rem;
    background: transparent;
    color: #6B7280;
    font-size: 0.8125rem;
    font-weight: 500;
    border: 1px solid rgba(107, 114, 128, 0.25);
    cursor: pointer;
    transition: color 150ms ease, border-color 150ms ease;
    white-space: nowrap;
  }

  .btn-dismiss:hover {
    color: #374151;
    border-color: rgba(107, 114, 128, 0.4);
  }
</style>
