import { writable, derived } from "svelte/store";
import type { ProjectSummary } from "$lib/types";

/** Reactive list of the projects loaded from the SQLite database. */
export const projects = writable<ProjectSummary[]>([]);

/** Total number of projects. */
export const projectCount = derived(projects, ($p) => $p.length);

/** `true` when no project exists. */
export const hasNoProjects = derived(projects, ($p) => $p.length === 0);
