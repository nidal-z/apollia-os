import { writable, derived } from "svelte/store";
import type { ProjectSummary } from "$lib/types";

/** Liste réactive des projets chargés depuis la base SQLite. */
export const projects = writable<ProjectSummary[]>([]);

/** Nombre total de projets. */
export const projectCount = derived(projects, ($p) => $p.length);

/** `true` quand aucun projet n'existe. */
export const hasNoProjects = derived(projects, ($p) => $p.length === 0);
