<script lang="ts">
  import { t } from "svelte-i18n";
  import { Button } from "$lib/components/ui/button";

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
    <Button variant="primary-gradient" size="sm" onclick={onresume}>
      {$t("onboarding_v2.resume_bar.resume_btn")}
    </Button>
    <Button variant="ghost" size="sm" onclick={ondismiss}>
      {$t("onboarding_v2.resume_bar.dismiss_btn")}
    </Button>
  </div>
</div>

<style>
  .resume-bar {
    height: 48px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 1.25rem;
    background: linear-gradient(90deg, hsl(var(--primary) / 0.06) 0%, hsl(var(--secondary) / 0.06) 100%);
    border-bottom: 1px solid hsl(var(--primary) / 0.1);
    flex-shrink: 0;
    gap: 0.75rem;
  }

  .step-label {
    font-size: 0.8125rem;
    font-weight: 500;
    color: hsl(var(--foreground) / 0.8);
  }

  .actions {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    flex-shrink: 0;
  }
</style>
