/**
 * Shared tour state used to coordinate the guided tour orchestrator with
 * target pages (Agents, Chat, Triggers).
 *
 * The orchestrator writes to these stores when a step is activated; target
 * pages read them to pre-populate forms and inputs.
 */
import { writable } from "svelte/store";
import type { TourInteraction } from "$lib/types";

/**
 * The interaction descriptor for the currently active tour step, or `null`
 * when no interactive step is in progress.
 *
 * Target pages use `interaction_type` to decide whether to pre-fill, and
 * `prefilled_data` for the actual values.
 */
export const tourPrefill = writable<TourInteraction | null>(null);

/**
 * An i18n key that overrides the step's default companion message.
 *
 * Set by the orchestrator when a step is auto-skipped (agent already active,
 * timeout) so the companion panel shows a contextual message instead of the
 * step title.
 */
export const tourCompanionOverride = writable<string | null>(null);
