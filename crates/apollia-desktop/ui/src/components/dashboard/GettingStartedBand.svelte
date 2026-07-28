<script lang="ts">
  /**
   * The getting-started band, full width above the dashboard bento.
   *
   * It is the single entry point of the tour system: nothing ever launches on
   * its own, so the band's visibility is not a layout detail. It carries the
   * call to action, the progress, and the five milestones.
   *
   * Milestones reflect real state, never clicks: creating a project ticks the
   * milestone whether or not the micro-tour was followed. The band is a read.
   */
  import { onMount } from "svelte";
  import { get } from "svelte/store";
  import { t } from "svelte-i18n";
  import { fade } from "svelte/transition";
  import { Check, MessageSquare, X } from "lucide-svelte";
  import { rm } from "$lib/design/motion";
  import { startTour } from "$lib/tour/engine";
  import {
    allComplete,
    completedCount,
    milestones,
    nextMilestone,
    refreshConnected,
  } from "$lib/tour/milestones";
  import { dismissBand, tourState } from "$lib/tour/persistence";

  /** Milliseconds the closing message stays before the band retires itself. */
  const CLOSING_MS = 5_000;

  let closing = $state(false);
  let kept = $state(false);
  let closeTimer: ReturnType<typeof setTimeout> | null = null;
  /**
   * Whether every milestone was already done when the band appeared.
   *
   * The closing sequence celebrates crossing the finish line, so it must fire on
   * the transition, never on mount. Without this, reopening the band from Help
   * with everything already ticked would show a farewell instead of the panel,
   * making that entry point useless.
   */
  let completeOnMount = $state(false);

  let visible = $derived(!$tourState.bandDismissed);
  let progress = $derived(($completedCount / $milestones.length) * 100);

  onMount(() => {
    completeOnMount = get(allComplete);
    void refreshConnected();
    return () => {
      if (closeTimer !== null) clearTimeout(closeTimer);
    };
  });

  // The band retires itself once the last milestone lands, leaving a way out for
  // anyone who would rather keep it. A discovery panel that outlives discovery
  // is noise on a mature dashboard.
  $effect(() => {
    if (!$allComplete || completeOnMount || kept || closing || !visible) return;
    closing = true;
    closeTimer = setTimeout(() => dismissBand(), CLOSING_MS);
  });

  function keepVisible(): void {
    kept = true;
    closing = false;
    if (closeTimer !== null) {
      clearTimeout(closeTimer);
      closeTimer = null;
    }
  }
</script>

{#if visible}
  <section
    class="glass-border relative overflow-hidden rounded-xl bg-card px-5 pb-4 pt-4 shadow-elev-2"
    data-testid="getting-started-card"
    aria-label={$t("tour.band.title")}
    transition:fade={rm({ duration: 200 })}
  >
    <span class="signature absolute inset-x-0 top-0 h-0.5" aria-hidden="true"></span>

    {#if closing}
      <div class="flex flex-wrap items-center gap-3">
        <span
          class="flex size-9 shrink-0 items-center justify-center rounded-full bg-success text-primary-foreground shadow-elev-2"
          aria-hidden="true"
        >
          <Check size={19} strokeWidth={3} />
        </span>
        <div class="min-w-0">
          <h2 class="text-heading-sm text-foreground">{$t("tour.band.done_title")}</h2>
          <p class="text-body-xs text-muted-foreground">{$t("tour.band.done_body")}</p>
        </div>
        <button
          type="button"
          class="ml-auto rounded-lg border border-border px-2.5 py-1.5 text-label-sm text-foreground transition-colors hover:bg-surface-2 focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-primary"
          onclick={keepVisible}
          data-testid="getting-started-keep"
        >
          {$t("tour.band.keep")}
        </button>
      </div>
    {:else}
      <div class="flex flex-wrap items-start gap-4">
        <div class="min-w-[13rem] flex-1">
          <div class="flex flex-wrap items-baseline gap-x-3 gap-y-1">
            <h2 class="text-heading-sm text-foreground">{$t("tour.band.title")}</h2>
            <!-- The landmarks tour has no milestone of its own: it situates the
                 frame rather than producing anything, so it lives here. -->
            <button
              type="button"
              class="rounded text-caption font-medium text-primary underline decoration-primary/30 underline-offset-2 transition-colors hover:decoration-primary focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-primary"
              onclick={() => void startTour("landmarks")}
              data-testid="getting-started-landmarks"
            >
              {$t("tour.band.landmarks")}
            </button>
          </div>
          <p class="mt-0.5 text-body-xs text-muted-foreground">{$t("tour.band.subtitle")}</p>
          <div class="mt-2.5 flex items-center gap-2.5">
            <span class="h-1 flex-1 overflow-hidden rounded-sm bg-muted">
              <span class="signature block h-full rounded-sm" style:width="{progress}%"></span>
            </span>
            <span class="text-overline text-muted-foreground">
              {$t("tour.band.progress", {
                values: { done: $completedCount, total: $milestones.length },
              })}
            </span>
          </div>
        </div>

        {#if $nextMilestone !== null}
          <button
            type="button"
            class="cta flex shrink-0 basis-80 items-center gap-3 rounded-xl border border-primary/30 px-3 py-2.5 text-left transition-all hover:-translate-y-px hover:border-primary/55 focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-primary"
            onclick={() => void startTour($nextMilestone.tour)}
            data-testid="getting-started-cta"
          >
            <span
              class="bg-primary-gradient flex size-8 shrink-0 items-center justify-center rounded-lg text-primary-foreground shadow-primary-md"
              aria-hidden="true"
            >
              <MessageSquare size={16} strokeWidth={2} />
            </span>
            <span class="min-w-0">
              <span class="block text-label-sm text-foreground">
                {$t(`tour.band.milestone.${$nextMilestone.id}.cta`)}
              </span>
              <span class="block text-caption text-muted-foreground">
                {$t(`tour.band.milestone.${$nextMilestone.id}.hint`)}
              </span>
            </span>
          </button>
        {/if}

        <button
          type="button"
          class="ml-auto shrink-0 rounded-md p-1.5 text-muted-foreground transition-colors hover:bg-surface-2 hover:text-foreground focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-primary"
          onclick={dismissBand}
          aria-label={$t("tour.band.hide")}
          data-testid="getting-started-hide"
        >
          <X size={14} strokeWidth={2} />
        </button>
      </div>

      <ul class="mt-3.5 grid grid-cols-2 gap-2 border-t border-border/60 pt-3 md:grid-cols-5">
        {#each $milestones as milestone (milestone.id)}
          <li>
            <button
              type="button"
              class="flex w-full items-start gap-2.5 rounded-lg border px-2.5 py-2 text-left transition-all hover:-translate-y-px hover:border-primary/35 focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-primary {milestone.done
                ? 'border-success/40 bg-success/5'
                : 'border-border bg-surface-1'}"
              onclick={() => void startTour(milestone.tour)}
              data-testid="getting-started-milestone-{milestone.id}"
            >
              <span
                class="mt-px flex size-[1.1875rem] shrink-0 items-center justify-center rounded-full border"
                class:border-border={!milestone.done}
                class:border-transparent={milestone.done}
                class:bg-success={milestone.done}
                class:text-primary-foreground={milestone.done}
                aria-hidden="true"
              >
                {#if milestone.done}
                  <Check size={10} strokeWidth={3.4} />
                {/if}
              </span>
              <span class="min-w-0">
                <span
                  class="block text-label-sm"
                  class:text-foreground={!milestone.done}
                  class:text-muted-foreground={milestone.done}
                  class:line-through={milestone.done}
                >
                  {$t(`tour.band.milestone.${milestone.id}.label`)}
                </span>
                <span class="block text-caption text-muted-foreground">
                  {$t(`tour.band.milestone.${milestone.id}.hint`)}
                </span>
              </span>
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  </section>
{/if}

<style>
  /* The brand signature gradient, used for the top hairline and the progress
     fill. Tailwind's `bg-primary-gradient` is 135deg; these run horizontally. */
  .signature {
    background: linear-gradient(90deg, hsl(var(--grad-a)), hsl(var(--grad-b)));
    transition: width var(--motion-slow) var(--ease-apple);
  }

  .cta {
    background: linear-gradient(
      135deg,
      hsl(var(--grad-a) / 0.1),
      hsl(var(--grad-b) / 0.06)
    );
  }

  @media (prefers-reduced-motion: reduce) {
    .signature {
      transition: none;
    }
  }
</style>
