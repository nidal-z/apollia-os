/**
 * What the end of the first launch hands over to the application: the first
 * task to open once the modal is closed, and the introduction the first
 * conversation shows above its composer.
 */
import { writable } from "svelte/store";
import type { FirstTaskIntro, FirstTaskPlan } from "./firstTask";

/**
 * The first task asked for by the modal as it closes; `null` when none is
 * waiting. `OnboardingFirstTask.svelte` takes it and clears it.
 */
export const firstTaskRequest = writable<FirstTaskPlan | null>(null);

/** The conversation the first launch closed on; `null` outside of it. */
export const firstTaskIntro = writable<FirstTaskIntro | null>(null);
