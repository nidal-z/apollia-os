<script lang="ts">
  // Construction-history scrubber for the plan DAG panel.
  //
  // Replays how the agent built the plan over time: a horizontal scrubber over
  // the recorded revisions, with step / play / pause controls and the reason
  // behind each change. The reconstructed state at the selected revision is
  // emitted upward through `onreconstruct` so the parent can drive the DAG to
  // show the past state rather than the live plan; the scrubber itself never
  // mutates the plan, it only reads the inviolable mutation trace.
  //
  // Data comes through the typed IPC wrapper (`listPlanMutations`); no `invoke`
  // here. A single revision (or none) shows an explicit empty state, never an
  // actionable scrubber. A corrupt mutation is skipped and surfaced as a visible
  // marker, never a crash. The replay timer is cleared on teardown so no orphan
  // interval survives the component.
  import { t } from "svelte-i18n";
  import { History, Play, Pause, ChevronLeft, ChevronRight } from "lucide-svelte";
  import { listPlanMutations, type PlanMutation } from "$lib/ipc/plan";
  import {
    reconstructPlanAt,
    revisionCount,
    type ReconstructResult,
  } from "./reconstructPlan";
  import { PLAN_MODE_KEYS } from "$lib/i18n/strings/planMode";
  import type { PlanStep } from "$lib/ipc/plan";

  type Props = {
    /** Chat session whose plan construction history is replayed. */
    sessionId: string;
    /** Emits the reconstructed steps at the current revision, or `null` when
     *  the scrubber is inactive (collapsed, empty, loading) so the parent
     *  falls back to the live plan. */
    onreconstruct?: (steps: PlanStep[] | null) => void;
  };
  let { sessionId, onreconstruct }: Props = $props();

  const REPLAY_INTERVAL_MS = 700;

  let open = $state(false);
  let loading = $state(false);
  let errored = $state(false);
  let mutations = $state<PlanMutation[]>([]);
  let revision = $state(0);
  let playing = $state(false);
  let timer: ReturnType<typeof setInterval> | null = null;

  const total = $derived(revisionCount(mutations));
  const hasHistory = $derived(total > 1);
  const reconstructed = $derived<ReconstructResult>(
    reconstructPlanAt(mutations, revision),
  );
  const currentReason = $derived(
    revision > 0 ? (mutations[revision - 1]?.reason ?? null) : null,
  );

  function stopTimer(): void {
    if (timer) {
      clearInterval(timer);
      timer = null;
    }
    playing = false;
  }

  // Push the reconstructed state up only while the scrubber is open and holds a
  // replayable history; otherwise yield control back to the live plan.
  $effect(() => {
    if (open && hasHistory) {
      onreconstruct?.(reconstructed.steps);
    } else {
      onreconstruct?.(null);
    }
  });

  // Load the mutation history when the panel opens or the session changes.
  $effect(() => {
    if (!open) return;
    const session = sessionId;
    let cancelled = false;
    loading = true;
    errored = false;
    void listPlanMutations(session)
      .then((list) => {
        if (cancelled) return;
        mutations = list;
        revision = list.length;
      })
      .catch(() => {
        if (cancelled) return;
        errored = true;
        mutations = [];
      })
      .finally(() => {
        if (!cancelled) loading = false;
      });
    return () => {
      cancelled = true;
    };
  });

  $effect(() => {
    return () => stopTimer();
  });

  function toggleOpen(): void {
    open = !open;
    if (!open) stopTimer();
  }

  function step(delta: number): void {
    stopTimer();
    revision = Math.max(0, Math.min(revision + delta, total));
  }

  function togglePlay(): void {
    if (playing) {
      stopTimer();
      return;
    }
    if (revision >= total) revision = 0;
    playing = true;
    timer = setInterval(() => {
      if (revision >= total) {
        stopTimer();
        return;
      }
      revision += 1;
    }, REPLAY_INTERVAL_MS);
  }

  function onScrubKey(event: KeyboardEvent): void {
    if (event.key === "ArrowLeft") {
      event.preventDefault();
      step(-1);
    } else if (event.key === "ArrowRight") {
      event.preventDefault();
      step(1);
    }
  }
</script>

<section class="border-t border-border px-3 py-2" data-testid="plan-history-scrubber">
  <button
    type="button"
    class="inline-flex items-center gap-1.5 text-caption font-medium text-muted-foreground transition-colors hover:text-foreground"
    aria-expanded={open}
    onclick={toggleOpen}
  >
    <History class="h-3.5 w-3.5" aria-hidden="true" />
    <span>{$t(open ? PLAN_MODE_KEYS.historyHide : PLAN_MODE_KEYS.historyShow)}</span>
  </button>

  {#if open}
    <div class="mt-2">
      {#if loading}
        <p class="text-caption text-muted-foreground">
          {$t(PLAN_MODE_KEYS.historyLoading)}
        </p>
      {:else if errored}
        <p class="text-caption text-destructive" role="alert">
          {$t(PLAN_MODE_KEYS.historyError)}
        </p>
      {:else if !hasHistory}
        <p class="text-caption text-muted-foreground" data-testid="plan-history-empty">
          {$t(PLAN_MODE_KEYS.historyEmpty)}
        </p>
      {:else}
        <div class="flex items-center gap-2">
          <button
            type="button"
            class="text-muted-foreground transition-colors hover:text-foreground disabled:opacity-40"
            aria-label={$t(PLAN_MODE_KEYS.historyStepBack)}
            disabled={revision <= 0}
            onclick={() => step(-1)}
          >
            <ChevronLeft class="h-4 w-4" aria-hidden="true" />
          </button>
          <input
            type="range"
            min="0"
            max={total}
            step="1"
            bind:value={revision}
            aria-label={$t(PLAN_MODE_KEYS.historyScrubberLabel)}
            aria-valuetext={$t(PLAN_MODE_KEYS.historyRevisionPosition, {
              values: { current: revision, total },
            })}
            onkeydown={onScrubKey}
            class="h-1 flex-1 cursor-pointer accent-primary"
          />
          <button
            type="button"
            class="text-muted-foreground transition-colors hover:text-foreground disabled:opacity-40"
            aria-label={$t(PLAN_MODE_KEYS.historyStepForward)}
            disabled={revision >= total}
            onclick={() => step(1)}
          >
            <ChevronRight class="h-4 w-4" aria-hidden="true" />
          </button>
          <button
            type="button"
            class="text-primary transition-opacity hover:opacity-80"
            aria-label={$t(playing ? PLAN_MODE_KEYS.historyPause : PLAN_MODE_KEYS.historyPlay)}
            onclick={togglePlay}
          >
            {#if playing}
              <Pause class="h-4 w-4" aria-hidden="true" />
            {:else}
              <Play class="h-4 w-4" aria-hidden="true" />
            {/if}
          </button>
        </div>

        <p class="mt-1 text-overline text-muted-foreground">
          {$t(PLAN_MODE_KEYS.historyRevisionPosition, {
            values: { current: revision, total },
          })}
        </p>

        {#if currentReason}
          <p class="mt-1 text-caption text-muted-foreground">
            <span class="font-medium">{$t(PLAN_MODE_KEYS.historyReasonLabel)}:</span>
            {currentReason}
          </p>
        {/if}

        {#if reconstructed.skipped.length > 0}
          <p class="mt-1 text-caption text-destructive" role="alert" data-testid="plan-history-corrupt">
            {$t(PLAN_MODE_KEYS.historyCorruptMarker, {
              values: { count: reconstructed.skipped.length },
            })}
          </p>
        {/if}
      {/if}
    </div>
  {/if}
</section>
