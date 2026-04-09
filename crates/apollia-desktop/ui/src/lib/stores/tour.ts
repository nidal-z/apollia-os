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

/**
 * The name of the agent whose detail panel should be opened programmatically,
 * or `null` when no programmatic open is requested.
 *
 * Written by the orchestrator for steps that require the detail panel to be
 * visible before the user interacts. The Agents page reads this store and opens
 * the corresponding panel. Reset to `null` at the start of each step activation.
 */
export const tourOpenAgentDetail = writable<string | null>(null);

/**
 * When set to `true`, the Chat page should open its "New Chat" picker
 * automatically. Reset to `false` once consumed.
 */
export const tourOpenChatPicker = writable(false);

/**
 * When set to an agent name, the tour orchestrator will stop that agent
 * before the step activates, then add a CSS ring-pulse class to its start
 * button. Reset to `null` at the start of each step.
 */
export const tourStopAgent = writable<string | null>(null);
