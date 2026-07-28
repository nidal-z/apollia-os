<script lang="ts">
  /**
   * Mounts the running tour. Replaces `OnboardingTourRunner` and
   * `FirstConnectionTourRunner`, which duplicated the same orchestration badly.
   *
   * Two presentations are served from here. `modal` dims the app and traps focus
   * inside the step card. `annotated` does neither: the first-result tour points
   * at a live approval card, and trapping focus would stop the user answering
   * the very thing the annotation is about.
   */
  import { onMount } from "svelte";
  import { t } from "svelte-i18n";
  import TourSpotlight from "./TourSpotlight.svelte";
  import TourStepCard from "./TourStepCard.svelte";
  import { Dialog, DialogFooter } from "$lib/components/ui/dialog";
  import { Button } from "$lib/components/ui/button";
  import {
    activeTour,
    cancelExit,
    nextStep,
    previousStep,
    remeasure,
    requestExit,
    skipTour,
  } from "$lib/tour/engine";

  let current = $derived($activeTour);
  let step = $derived(current === null ? null : (current.steps[current.index] ?? null));
  let isModal = $derived(current?.definition.presentation === "modal");

  onMount(() => {
    function onKeydown(event: KeyboardEvent): void {
      if ($activeTour === null) return;

      if (event.key === "Escape") {
        event.preventDefault();
        requestExit();
        return;
      }
      if (event.key === "ArrowRight") {
        event.preventDefault();
        void nextStep();
        return;
      }
      if (event.key === "ArrowLeft") {
        event.preventDefault();
        void previousStep();
      }
    }

    function onLayoutChange(): void {
      remeasure();
    }

    window.addEventListener("keydown", onKeydown);
    window.addEventListener("resize", onLayoutChange);
    window.addEventListener("scroll", onLayoutChange, { passive: true });

    return () => {
      window.removeEventListener("keydown", onKeydown);
      window.removeEventListener("resize", onLayoutChange);
      window.removeEventListener("scroll", onLayoutChange);
    };
  });
</script>

{#if current !== null && step !== null}
  {#if isModal}
    <TourSpotlight rect={current.rect} onoverlayclick={requestExit} />
  {/if}

  <TourStepCard
    title={$t(step.titleKey)}
    description={$t(step.bodyKey)}
    stepIndex={current.index}
    totalSteps={current.steps.length}
    targetRect={current.rect}
    presentation={current.definition.presentation}
    onnext={() => void nextStep()}
    onprev={() => void previousStep()}
    onskip={requestExit}
  />

  <!-- Not `ConfirmDialog`: leaving a tour is not a destructive action, and that
       component hardcodes a destructive confirm button. -->
  <Dialog
    open={current.confirmingExit}
    onclose={cancelExit}
    size="sm"
    title={$t("tour.chrome.exit_title")}
    data-testid="tour-exit"
  >
    <p class="text-body-sm text-muted-foreground">{$t("tour.chrome.exit_body")}</p>
    <DialogFooter>
      <Button variant="outline" onclick={cancelExit} data-testid="tour-exit-cancel">
        {$t("tour.chrome.exit_cancel")}
      </Button>
      <Button variant="primary-gradient" onclick={skipTour} data-testid="tour-exit-confirm">
        {$t("tour.chrome.exit_confirm")}
      </Button>
    </DialogFooter>
  </Dialog>
{/if}
