import { writable } from "svelte/store";

/** Controls the visibility of the onboarding wizard. */
export const showOnboarding = writable<boolean>(false);
