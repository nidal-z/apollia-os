<script lang="ts">
  /**
   * Single card rendered by `NextStepsPanel`.
   *
   * Keyboard-focusable, `aria-live`-friendly, and delegates dismiss /
   * feedback to the parent panel. The CTA click dispatches `onaction` -
   * the parent resolves the payload against the allowlist (`navigate`
   * routes are mapped to the Route enum; `invoke` commands are awaited
   * via Tauri).
   */
  import { t } from "svelte-i18n";
  import { X, ThumbsUp, ThumbsDown } from "lucide-svelte";
  import type { NextStep } from "$lib/stores/nextSteps";

  interface Props {
    step: NextStep;
    feedback?: "useful" | "not_useful";
    onaction: (step: NextStep) => void;
    ondismiss: (id: string) => void;
    onfeedback: (id: string, value: "useful" | "not_useful") => void;
  }

  let { step, feedback, onaction, ondismiss, onfeedback }: Props = $props();

  function handleActivate(event: KeyboardEvent | MouseEvent) {
    if (event instanceof KeyboardEvent) {
      if (event.key !== "Enter" && event.key !== " ") return;
      event.preventDefault();
    }
    onaction(step);
  }
</script>

<article
  class="group relative flex flex-col gap-2 rounded-md border border-border/30 bg-card/60 p-3 transition-colors hover:border-primary/40 focus-within:border-primary/60"
  data-testid="next-step-card-{step.id}"
>
  <button
    type="button"
    class="absolute right-2 top-2 text-muted-foreground/60 opacity-0 transition-opacity hover:text-foreground focus:opacity-100 group-hover:opacity-100"
    aria-label={$t("next_steps.dismiss")}
    data-testid="next-step-dismiss-{step.id}"
    onclick={() => ondismiss(step.id)}
  >
    <X size={14} />
  </button>

  <div
    role="button"
    tabindex="0"
    class="flex cursor-pointer flex-col gap-1 pr-6 text-left"
    data-testid="next-step-body-{step.id}"
    onclick={handleActivate}
    onkeydown={handleActivate}
  >
    <h3 class="text-sm font-medium text-foreground">{step.title}</h3>
    <p class="text-xs leading-relaxed text-muted-foreground">{step.description}</p>
  </div>

  <div class="flex items-center justify-between pt-1">
    <button
      type="button"
      class="rounded-md bg-primary/10 px-2.5 py-1 text-xs font-medium text-primary transition-colors hover:bg-primary/20"
      data-testid="next-step-cta-{step.id}"
      onclick={() => onaction(step)}
    >
      {step.actionButton.label}
    </button>

    <div class="flex items-center gap-1" role="group" aria-label={$t("next_steps.feedback_label")}>
      <button
        type="button"
        class="rounded p-1 text-muted-foreground/60 transition-colors hover:text-success {feedback === 'useful' ? 'text-success' : ''}"
        aria-pressed={feedback === "useful"}
        aria-label={$t("next_steps.feedback_useful")}
        data-testid="next-step-feedback-useful-{step.id}"
        onclick={() => onfeedback(step.id, "useful")}
      >
        <ThumbsUp size={12} />
      </button>
      <button
        type="button"
        class="rounded p-1 text-muted-foreground/60 transition-colors hover:text-destructive {feedback === 'not_useful' ? 'text-destructive' : ''}"
        aria-pressed={feedback === "not_useful"}
        aria-label={$t("next_steps.feedback_not_useful")}
        data-testid="next-step-feedback-not-useful-{step.id}"
        onclick={() => onfeedback(step.id, "not_useful")}
      >
        <ThumbsDown size={12} />
      </button>
    </div>
  </div>
</article>
