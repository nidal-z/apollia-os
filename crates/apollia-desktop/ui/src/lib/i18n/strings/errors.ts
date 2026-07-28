/**
 * Error-category string catalog.
 *
 * Typed index over the `errors.*` i18n block: for each category, the i18n key
 * stem plus the presentation metadata (Lucide icon, banner tone) a surface
 * needs to render a `HumanizedError`. The copies live in `en.json` / `fr.json`
 * under `errors.<segment>.{title,friendly_message,suggested_action}`.
 *
 * The `ErrorCategory` union is owned by the humanizer (`$lib/errors/humanize`);
 * this file is the presentational companion, kept next to the other typed
 * i18n catalogs.
 */
import type { Icon } from "lucide-svelte";
import {
  ShieldAlert,
  PlugZap,
  AlertTriangle,
  KeyRound,
  Timer,
  SearchX,
  GitMerge,
  CircleAlert,
} from "lucide-svelte";
import type { ErrorCategory } from "$lib/errors/humanize";

/** Banner tone reused from the operator/ui banner tone vocabulary. */
export type ErrorTone = "danger" | "warning" | "info";

export interface ErrorCategoryEntry {
  category: ErrorCategory;
  /** i18n key stem, e.g. `errors.permission`. Append `.title` etc. */
  keyStem: string;
  /** Default Lucide icon for the category. */
  icon: typeof Icon;
  /** Banner tone. */
  tone: ErrorTone;
}

export const ERROR_CATEGORIES: Record<ErrorCategory, ErrorCategoryEntry> = {
  permission: {
    category: "permission",
    keyStem: "errors.permission",
    icon: ShieldAlert,
    tone: "warning",
  },
  ipc: {
    category: "ipc",
    keyStem: "errors.ipc",
    icon: PlugZap,
    tone: "danger",
  },
  validation: {
    category: "validation",
    keyStem: "errors.validation",
    icon: AlertTriangle,
    tone: "warning",
  },
  auth: {
    category: "auth",
    keyStem: "errors.auth",
    icon: KeyRound,
    tone: "warning",
  },
  "rate-limit": {
    category: "rate-limit",
    keyStem: "errors.rate_limit",
    icon: Timer,
    tone: "info",
  },
  "not-found": {
    category: "not-found",
    keyStem: "errors.not_found",
    icon: SearchX,
    tone: "info",
  },
  conflict: {
    category: "conflict",
    keyStem: "errors.conflict",
    icon: GitMerge,
    tone: "warning",
  },
  generic: {
    category: "generic",
    keyStem: "errors.generic",
    icon: CircleAlert,
    tone: "danger",
  },
};

/** Ordered list - drives showcase iteration. */
export const ERROR_CATEGORY_ORDER: ErrorCategory[] = [
  "permission",
  "ipc",
  "validation",
  "auth",
  "rate-limit",
  "not-found",
  "conflict",
  "generic",
];
