/**
 * Template gallery — frontend types + invoke wrappers.
 *
 * The backend lives in `apollia-runtime/src/templates/` and exposes three
 * Tauri commands: `templates_list`, `templates_get`, `templates_instantiate`.
 * This module mirrors the returned shapes in TypeScript and provides thin
 * wrappers so components never talk to `invoke` directly.
 */
import { invoke } from "@tauri-apps/api/core";

export type TemplateKind = "automation" | "agent";
export type TemplateCategory =
  | "productivity"
  | "dev"
  | "communication"
  | "creative"
  | "analysis"
  | "system";
export type TemplateDifficulty = "simple" | "intermediate" | "advanced";
export type TemplateSource = "official" | "community";

export interface TemplateMeta {
  id: string;
  kind: TemplateKind;
  category: TemplateCategory;
  difficulty: TemplateDifficulty;
  source: TemplateSource;
  author: string;
  title: string;
  description: string;
  path: string;
  dependencies: string[];
}

export interface TemplateFull extends TemplateMeta {
  /** Raw TOML body — surfaced in the "Show code" dialog (builder only). */
  body: string;
}

export interface TemplateInstantiation {
  template_id: string;
  kind: TemplateKind;
  suggested_name: string;
  body: string;
}

export async function listTemplates(): Promise<TemplateMeta[]> {
  return invoke<TemplateMeta[]>("templates_list");
}

export async function getTemplate(id: string): Promise<TemplateFull> {
  return invoke<TemplateFull>("templates_get", { id });
}

export async function instantiateTemplate(
  id: string,
): Promise<TemplateInstantiation> {
  return invoke<TemplateInstantiation>("templates_instantiate", { id });
}

/** The six user-facing categories, in the order they appear in filters. */
export const CATEGORIES: TemplateCategory[] = [
  "productivity",
  "dev",
  "communication",
  "creative",
  "analysis",
  "system",
];

export const DIFFICULTIES: TemplateDifficulty[] = [
  "simple",
  "intermediate",
  "advanced",
];

export const SOURCES: TemplateSource[] = ["official", "community"];
