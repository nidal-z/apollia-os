/**
 * Shared option lists, sensitive-key set, and small pure helpers for the
 * profile form. Kept out of the Svelte components so the (verbose) static
 * config does not bloat the section files and can be unit-reasoned in one place.
 *
 * The profile is a flat key/value store; the form below mirrors PROFILE_SCHEMA
 * (declared in Rust) by hand. Editing a sensitive key does NOT re-derive the
 * permission rules: the user must rerun onboarding to apply matching proposals.
 */

export interface Option {
  v: string;
  labelKey: string;
}

export interface RadioOption {
  v: string;
  labelKey: string;
  hintKey?: string;
}

/** Keys whose value impacts agent behaviour and tool permissions. */
export const SENSITIVE_KEYS = new Set<string>([
  "constraints.sovereignty",
  "constraints.compliance",
  "agents.hitl",
  "tech.integrations",
]);

/**
 * Optional Tier 2 fields. Once name + role are set (calibration done) but some
 * of these are still empty, the enrichment banner offers to resume onboarding.
 */
export const TIER2_KEYS = [
  "goals",
  "domain.sector",
  "domain.team_size",
  "tech.stack",
  "tech.proficiency",
  "tools.daily",
  "tech.integrations",
  "preferences.language",
  "preferences.llm",
  "agents.domains",
  "agents.trigger",
  "constraints.compliance",
];

export const SECTORS: Option[] = [
  { v: "", labelKey: "settings.profile.option.none" },
  { v: "fintech", labelKey: "settings.profile.sector.fintech" },
  { v: "sante", labelKey: "settings.profile.sector.sante" },
  { v: "ecommerce", labelKey: "settings.profile.sector.ecommerce" },
  { v: "industrie", labelKey: "settings.profile.sector.industrie" },
  { v: "education", labelKey: "settings.profile.sector.education" },
  { v: "autre", labelKey: "settings.profile.sector.autre" },
];

export const TEAM_SIZES: Option[] = [
  { v: "", labelKey: "settings.profile.option.none" },
  { v: "solo", labelKey: "settings.profile.team_size.solo" },
  { v: "2-5", labelKey: "settings.profile.team_size.2_5" },
  { v: "6-20", labelKey: "settings.profile.team_size.6_20" },
  { v: "20+", labelKey: "settings.profile.team_size.20_plus" },
];

export const PROFICIENCY: Option[] = [
  { v: "", labelKey: "settings.profile.option.none" },
  { v: "debutant", labelKey: "settings.profile.proficiency.debutant" },
  { v: "intermediaire", labelKey: "settings.profile.proficiency.intermediaire" },
  { v: "avance", labelKey: "settings.profile.proficiency.avance" },
  { v: "expert", labelKey: "settings.profile.proficiency.expert" },
];

export const LANGUAGES: Option[] = [
  { v: "", labelKey: "settings.profile.option.none" },
  { v: "fr", labelKey: "settings.profile.language.fr" },
  { v: "en", labelKey: "settings.profile.language.en" },
];

export const LLM_BACKENDS: Option[] = [
  { v: "", labelKey: "settings.profile.option.none" },
  { v: "local", labelKey: "settings.profile.llm.local" },
  { v: "ollama", labelKey: "settings.profile.llm.ollama" },
  { v: "anthropic", labelKey: "settings.profile.llm.anthropic" },
  { v: "openai", labelKey: "settings.profile.llm.openai" },
  { v: "bedrock", labelKey: "settings.profile.llm.bedrock" },
  { v: "vertex", labelKey: "settings.profile.llm.vertex" },
];

export const HITL_OPTIONS: RadioOption[] = [
  {
    v: "always",
    labelKey: "settings.profile.hitl.always.label",
    hintKey: "settings.profile.hitl.always.hint",
  },
  {
    v: "critical-only",
    labelKey: "settings.profile.hitl.critical_only.label",
    hintKey: "settings.profile.hitl.critical_only.hint",
  },
  {
    v: "never",
    labelKey: "settings.profile.hitl.never.label",
    hintKey: "settings.profile.hitl.never.hint",
  },
];

export const SOVEREIGNTY_OPTIONS: RadioOption[] = [
  {
    v: "local-only",
    labelKey: "settings.profile.sovereignty.local_only.label",
    hintKey: "settings.profile.sovereignty.local_only.hint",
  },
  {
    v: "local-preferred",
    labelKey: "settings.profile.sovereignty.local_preferred.label",
    hintKey: "settings.profile.sovereignty.local_preferred.hint",
  },
  {
    v: "cloud-ok",
    labelKey: "settings.profile.sovereignty.cloud_ok.label",
    hintKey: "settings.profile.sovereignty.cloud_ok.hint",
  },
];

export const TRIGGER_OPTIONS: RadioOption[] = [
  { v: "manuel", labelKey: "settings.profile.trigger.manuel" },
  { v: "planifie", labelKey: "settings.profile.trigger.planifie" },
  { v: "evenementiel", labelKey: "settings.profile.trigger.evenementiel" },
];

export const COMPLIANCE = ["RGPD", "HIPAA", "SOC2"];
export const INTEGRATIONS = ["GitHub", "Slack", "Notion", "Gmail"];

/** Split a comma-separated list value into trimmed, non-empty items. */
export function listItems(value: string): string[] {
  return value
    .split(",")
    .map((s) => s.trim())
    .filter(Boolean);
}

/** True when the comma-separated `value` already contains `entry`. */
export function listIncludes(value: string, entry: string): boolean {
  return listItems(value).includes(entry);
}

/** Toggle `entry` in the comma-separated `value`, returning the new value. */
export function toggleList(value: string, entry: string, checked: boolean): string {
  const set = new Set(listItems(value));
  if (checked) set.add(entry);
  else set.delete(entry);
  return Array.from(set).join(", ");
}

/**
 * The reactive facade every profile section receives. Sections read `values`
 * / `sources` (deeply reactive `$state` proxies owned by the page) and mutate
 * through `set`, so all edits funnel through one dirty-tracking path.
 */
export interface ProfileFieldApi {
  values: Record<string, string>;
  sources: Record<string, string>;
  set: (key: string, value: string) => void;
}

/** Resolve a selected option value to its i18n label key, or null when unset. */
export function optionLabelKey(list: Option[], value: string): string | null {
  if (!value) return null;
  return list.find((o) => o.v === value)?.labelKey ?? null;
}

/** Map a raw `written_by` provenance to its i18n label key, or null. */
export function sourceLabelKey(src: string | undefined): string | null {
  if (!src) return null;
  if (src === "onboarding") return "settings.profile.source.onboarding";
  if (src === "user") return "settings.profile.source.user";
  if (src.startsWith("agent:")) return "settings.profile.source.agent";
  return null;
}
