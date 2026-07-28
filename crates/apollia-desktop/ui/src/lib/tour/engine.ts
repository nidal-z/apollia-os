/**
 * The tour engine.
 *
 * Owns which tour is running, which step is current, and where that step's
 * anchor sits. One tour at a time: a request made while another tour runs is
 * refused rather than queued.
 *
 * Pre-flight is the reason the step counter can be trusted. Before the first
 * step shows, every anchor is resolved; the ones that fail are dropped unless
 * marked `optional`, in which case they are kept and fall back to the
 * anchorless presentation. A tour whose steps all fail never starts, so a future
 * refactor that breaks the anchors degrades visibly instead of painting a black
 * overlay over an empty page.
 */
import { derived, get, writable, type Readable } from "svelte/store";
import { navigateTo } from "$lib/stores/navigation";
import { tourById } from "./catalog";
import { measureAnchor, waitForAnchor } from "./anchor";
import { runStateOf, setRunState, tourState } from "./persistence";
import type { ActiveTour, TourDefinition, TourId, TourStep } from "./types";

interface EngineState {
  definition: TourDefinition | null;
  steps: readonly TourStep[];
  index: number;
  rect: DOMRect | null;
  confirmingExit: boolean;
}

const IDLE: EngineState = {
  definition: null,
  steps: [],
  index: 0,
  rect: null,
  confirmingExit: false,
};

const state = writable<EngineState>(IDLE);

/** The running tour, or `null` when nothing is running. */
export const activeTour: Readable<ActiveTour | null> = derived(state, ($state) =>
  $state.definition === null
    ? null
    : {
        definition: $state.definition,
        steps: $state.steps,
        index: $state.index,
        rect: $state.rect,
        confirmingExit: $state.confirmingExit,
      },
);

/** Aborts the pending anchor wait when the step changes or the tour ends. */
let waitController: AbortController | null = null;

/** Guards the async window inside {@link startTour} against a double start. */
let starting = false;

function abortPendingWait(): void {
  waitController?.abort();
  waitController = null;
}

/**
 * Whether a step survives pre-flight.
 *
 * A step with no anchor always survives. So does an optional one, which falls
 * back to the anchorless presentation, and one that waits for its anchor, whose
 * whole purpose is to fire on an element that does not exist yet. Everything
 * else must resolve now or be dropped, so the counter stays honest.
 *
 * Exported as a pure predicate so it can be tested without a DOM.
 */
export function shouldRetainStep(step: TourStep, anchorResolves: boolean): boolean {
  if (step.anchor === null) return true;
  if (step.optional === true || step.awaitAnchor === true) return true;
  return anchorResolves;
}

/**
 * Applies {@link shouldRetainStep} against the live DOM.
 *
 * A step whose retention depends on its anchor is given a bounded wait rather
 * than an instant read. A tour navigates to its route immediately before
 * pre-flight and the destination paints on a later frame, so an instant read
 * would drop every step anchored on route content and leave only the anchorless
 * ones. Steps kept regardless of their anchor (none, optional, awaitAnchor) are
 * retained without waiting, so a genuinely removed anchor still drops once the
 * budget elapses and the counter stays honest.
 */
async function preflight(definition: TourDefinition): Promise<readonly TourStep[]> {
  const retained: TourStep[] = [];
  for (const step of definition.steps) {
    if (step.anchor === null || step.optional === true || step.awaitAnchor === true) {
      retained.push(step);
      continue;
    }
    if (shouldRetainStep(step, (await waitForAnchor(step.anchor)) !== null)) {
      retained.push(step);
    }
  }
  return retained;
}

/**
 * Measures the current step and, when it waits for its anchor, keeps watching
 * until the element shows up.
 */
async function settle(step: TourStep): Promise<void> {
  abortPendingWait();

  state.update((current) => ({ ...current, rect: measureAnchor(step.anchor) }));
  if (step.anchor === null || step.awaitAnchor !== true) return;
  if (get(state).rect !== null) return;

  const controller = new AbortController();
  waitController = controller;

  // No budget: the approval card may take a while, or may never come. The
  // absence of a card is an accepted outcome, not a failure to report.
  await waitForAnchor(step.anchor, { budgetMs: null, signal: controller.signal });
  if (controller.signal.aborted) return;

  state.update((current) =>
    current.definition === null ? current : { ...current, rect: measureAnchor(step.anchor) },
  );
}

/**
 * Starts a tour.
 *
 * Returns `false` when another tour is already running, or when pre-flight
 * retained no step at all.
 */
export async function startTour(id: TourId, options: { resume?: boolean } = {}): Promise<boolean> {
  if (get(state).definition !== null || starting) return false;
  starting = true;
  try {
    const definition = tourById(id);
    if (definition.route !== null) navigateTo(definition.route);
    definition.onStart?.();

    const steps = await preflight(definition);
    if (steps.length === 0) return false;

    const stored = runStateOf(get(tourState), id);
    const resumeIndex =
      options.resume === true && stored.status === "in_progress"
        ? Math.min(stored.index, steps.length - 1)
        : 0;

    state.set({ definition, steps, index: resumeIndex, rect: null, confirmingExit: false });
    setRunState(id, { status: "in_progress", index: resumeIndex });

    const step = steps[resumeIndex];
    if (step !== undefined) await settle(step);
    return true;
  } finally {
    starting = false;
  }
}

/** Moves to the next step, finishing the tour past the last one. */
export async function nextStep(): Promise<void> {
  const current = get(state);
  if (current.definition === null) return;

  const next = current.index + 1;
  if (next >= current.steps.length) {
    finishTour();
    return;
  }

  state.update((s) => ({ ...s, index: next, rect: null }));
  setRunState(current.definition.id, { status: "in_progress", index: next });

  const step = current.steps[next];
  if (step !== undefined) await settle(step);
}

/** Moves back one step. No-op on the first step. */
export async function previousStep(): Promise<void> {
  const current = get(state);
  if (current.definition === null || current.index === 0) return;

  const previous = current.index - 1;
  state.update((s) => ({ ...s, index: previous, rect: null }));
  setRunState(current.definition.id, { status: "in_progress", index: previous });

  const step = current.steps[previous];
  if (step !== undefined) await settle(step);
}

/** Re-measures the current step, after a resize or a layout shift. */
export function remeasure(): void {
  const current = get(state);
  if (current.definition === null) return;
  const step = current.steps[current.index];
  if (step === undefined) return;
  state.update((s) => ({ ...s, rect: measureAnchor(step.anchor) }));
}

/** Shows the exit confirmation. */
export function requestExit(): void {
  state.update((current) =>
    current.definition === null ? current : { ...current, confirmingExit: true },
  );
}

/** Dismisses the exit confirmation and stays in the tour. */
export function cancelExit(): void {
  state.update((current) => ({ ...current, confirmingExit: false }));
}

/** Leaves the tour, keeping the current index so it can be resumed. */
export function skipTour(): void {
  const current = get(state);
  if (current.definition === null) return;
  setRunState(current.definition.id, { status: "skipped", index: current.index });
  abortPendingWait();
  state.set(IDLE);
}

/** Marks the tour done and closes it. */
export function finishTour(): void {
  const current = get(state);
  if (current.definition === null) return;
  setRunState(current.definition.id, { status: "done", index: current.steps.length });
  abortPendingWait();
  state.set(IDLE);
}
