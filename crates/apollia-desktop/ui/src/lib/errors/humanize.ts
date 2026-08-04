/**
 * Humanize raw backend / IPC errors into operator-friendly guidance.
 *
 * Pattern-based and deterministic: no network, no Meta-LLM. A raw error string
 * is matched to a category, and the display strings come from i18n
 * (`errors.<category>.*`). `humanize()` ALWAYS returns a `HumanizedError` -
 * unmatched input falls back to the generic category - so callers never have
 * to handle `undefined`.
 *
 * The raw text is preserved on `detail` for a "Details" disclosure.
 *
 * This module is i18n-agnostic: the caller passes a translator, so the mapper
 * stays pure and unit-testable. `reportError` wires the real `svelte-i18n`
 * formatter.
 */

/** Coarse error family - drives icon/tone in the catalog and the i18n stem. */
export type ErrorCategory =
  | "permission"
  | "ipc"
  | "validation"
  | "auth"
  | "rate-limit"
  | "not-found"
  | "conflict"
  | "generic";

/**
 * Minimal shape of the translator this module needs: key in, string out. The
 * category copies take no interpolation, so a single-argument signature keeps
 * the `svelte-i18n` `MessageFormatter` (and a plain identity fn in tests)
 * assignable without variance friction.
 */
export type ErrorTranslate = (key: string) => string;

export interface HumanizedError {
  /** Short, title-cased headline. */
  title: string;
  /** 1-2 sentence explanation in plain language. */
  friendly_message: string;
  /** One concrete action the operator can take right now. */
  suggested_action: string;
  /** Coarse family this error was sorted into. */
  category: ErrorCategory;
  /** Matched pattern key for telemetry. Undefined on the generic fallback. */
  code?: string;
  /** Optional deep link to docs. */
  learn_more_url?: string;
  /** Raw error text, kept for a "Details" disclosure. */
  detail?: string;
}

interface ErrorMatcher {
  code: string;
  category: ErrorCategory;
  regex: RegExp;
  learn_more_url?: string;
  /** Legacy resolved English strings, permission patterns only (backward compat). */
  legacy?: {
    title: string;
    friendly_message: string;
    suggested_action: string;
  };
}

/**
 * Root of the published documentation site, built from `docs/site` by CI.
 *
 * Every `learn_more_url` below is a page that exists in that build. The base
 * used to point at the repository wiki, retired when the three corpora were
 * consolidated onto this site, so all five deep links shipped dead.
 */
const DOCS_BASE = "https://docs.apollia.fr";

/** i18n segment per category (dot-path safe). */
const CATEGORY_KEY: Record<ErrorCategory, string> = {
  permission: "permission",
  ipc: "ipc",
  validation: "validation",
  auth: "auth",
  "rate-limit": "rate_limit",
  "not-found": "not_found",
  conflict: "conflict",
  generic: "generic",
};

/**
 * Ordered matchers - first hit wins, so put specific patterns before broad
 * ones. Permission patterns carry `legacy` English used by the backward-compat
 * `permissionErrorHumanize` shim; the general `humanize` uses i18n per category.
 */
const MATCHERS: ErrorMatcher[] = [
  {
    code: "POLICY_DENIED",
    category: "permission",
    regex: /permission denied by policy|policy denied|policy violation/i,
    learn_more_url: `${DOCS_BASE}/operator-help/controle/approuver-ou-refuser-une-action`,
    legacy: {
      title: "Blocked by policy",
      friendly_message:
        "Your Apollia policy refuses this action for this agent or tool.",
      suggested_action:
        "Review the matching rule in Settings > Permissions, or create an exception.",
    },
  },
  {
    code: "EACCES",
    category: "permission",
    regex: /\bEACCES\b|permission denied/i,
    learn_more_url: `${DOCS_BASE}/operator-help/controle/configurer-les-permissions-de-fichiers`,
    legacy: {
      title: "Permission denied",
      friendly_message:
        "The system blocked this action because the current process doesn't have access to the target resource.",
      suggested_action:
        "Check that the path exists and that Apollia has read/write rights on it.",
    },
  },
  {
    code: "EPERM",
    category: "permission",
    regex: /\bEPERM\b|operation not permitted/i,
    learn_more_url: `${DOCS_BASE}/operator-help/controle/configurer-les-permissions-de-fichiers`,
    legacy: {
      title: "Operation not permitted",
      friendly_message:
        "The operating system refused this call, typically because of file flags, immutability, or sandbox policy.",
      suggested_action:
        "Try running the action on a file you own, or lift the system-level protection.",
    },
  },
  {
    code: "ENOENT",
    category: "permission",
    regex: /\bENOENT\b|no such file or directory/i,
    legacy: {
      title: "File not found",
      friendly_message: "The requested path does not exist on disk.",
      suggested_action:
        "Verify the path spelling, or ask the agent to create it first.",
    },
  },
  {
    code: "EISDIR",
    category: "permission",
    regex: /\bEISDIR\b|is a directory/i,
    legacy: {
      title: "Path is a directory",
      friendly_message:
        "The action expected a file but received a directory path.",
      suggested_action: "Point the agent at a file inside the directory.",
    },
  },
  {
    code: "ENOSPC",
    category: "permission",
    regex: /\bENOSPC\b|no space left on device/i,
    legacy: {
      title: "No disk space",
      friendly_message: "The target volume is full.",
      suggested_action: "Free up disk space, then retry the action.",
    },
  },
  {
    code: "TIMEOUT",
    category: "permission",
    regex: /\b(timeout|timed out|etimedout)\b/i,
    legacy: {
      title: "Action timed out",
      friendly_message:
        "The operation did not complete within the allowed time window.",
      suggested_action:
        "Retry, or raise the timeout budget for this tool in Settings.",
    },
  },
  {
    code: "NETWORK",
    category: "permission",
    regex: /\b(econnrefused|enetunreach|enotfound|dns)\b/i,
    legacy: {
      title: "Network unreachable",
      friendly_message:
        "Apollia could not reach the remote endpoint - the host is down, offline, or unresolvable.",
      suggested_action:
        "Check your connection and the endpoint URL, then retry.",
    },
  },
  {
    code: "BUDGET_EXCEEDED",
    category: "permission",
    regex: /budget exceeded|stepbudget|step budget/i,
    learn_more_url: `${DOCS_BASE}/reference/sdk/budget`,
    legacy: {
      title: "Agent budget exhausted",
      friendly_message:
        "This agent has reached its StepBudget and cannot take more actions in this run.",
      suggested_action:
        "Raise the budget in the agent manifest, or start a fresh run.",
    },
  },
  {
    code: "SANDBOX_VIOLATION",
    category: "permission",
    regex: /sandbox (violation|denied|blocked)/i,
    learn_more_url: `${DOCS_BASE}/explanation/agent-trust-model`,
    legacy: {
      title: "Blocked by sandbox",
      friendly_message:
        "The tool tried to escape its sandbox and was stopped by the runtime.",
      suggested_action:
        "This is almost always intentional - review what the agent tried to do before granting access.",
    },
  },
  {
    code: "AUTH",
    category: "auth",
    regex: /\b(401|403)\b|unauthorized|forbidden|oauth|invalid[_ ]?token|token expired|not authenticated/i,
  },
  {
    code: "RATE_LIMIT",
    category: "rate-limit",
    regex: /\b429\b|rate limit|too many requests|throttl/i,
  },
  {
    code: "NOT_FOUND",
    category: "not-found",
    regex: /\b404\b|resource not found|\bnot found\b/i,
  },
  {
    code: "CONFLICT",
    category: "conflict",
    regex: /\b409\b|conflict|already exists|duplicate/i,
  },
  {
    code: "VALIDATION",
    category: "validation",
    regex: /\b400\b|invalid|validation|malformed|required field|bad request/i,
  },
  {
    code: "IPC",
    category: "ipc",
    regex: /\b(ipc|invoke|tauri)\b|\b500\b|internal server error|command failed|backend unavailable/i,
  },
];

function rawText(raw: unknown): string {
  if (typeof raw === "string") return raw;
  if (raw instanceof Error) return raw.message;
  if (raw === null || raw === undefined) return "";
  return String(raw);
}

/**
 * Map any raw error to a display-ready `HumanizedError`. Always returns a
 * result; unmatched input uses the generic category.
 */
export function humanize(raw: unknown, translate: ErrorTranslate): HumanizedError {
  const detail = rawText(raw);
  const match = detail ? MATCHERS.find((m) => m.regex.test(detail)) : undefined;
  const category: ErrorCategory = match?.category ?? "generic";
  const stem = `errors.${CATEGORY_KEY[category]}`;
  return {
    title: translate(`${stem}.title`),
    friendly_message: translate(`${stem}.friendly_message`),
    suggested_action: translate(`${stem}.suggested_action`),
    category,
    code: match?.code,
    learn_more_url: match?.learn_more_url,
    detail: detail || undefined,
  };
}

/**
 * Backward-compat permission mapper. Returns the legacy English payload for
 * permission patterns only, or `undefined` when nothing matches (callers then
 * fall back to the Meta-LLM humanizer). Kept so existing consumers and tests
 * of the former `permissions/errorHumanize` module keep working.
 */
export function permissionErrorHumanize(
  raw: string | null | undefined,
): HumanizedError | undefined {
  if (!raw || typeof raw !== "string") return undefined;
  for (const m of MATCHERS) {
    if (m.category !== "permission" || !m.legacy) continue;
    if (m.regex.test(raw)) {
      return {
        code: m.code,
        category: "permission",
        title: m.legacy.title,
        friendly_message: m.legacy.friendly_message,
        suggested_action: m.legacy.suggested_action,
        learn_more_url: m.learn_more_url,
      };
    }
  }
  return undefined;
}

/** Exposed for tests. */
export const _MATCHERS = MATCHERS;
