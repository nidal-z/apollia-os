/**
 * Humanize low-level backend permission errors into operator-friendly
 * guidance.
 *
 * The mapper is intentionally pattern-based — Meta-LLM fallback happens
 * elsewhere, this module stays fast, offline, and deterministic.
 */

export interface HumanizedError {
  /** Short, title-cased headline (e.g. "Permission denied"). */
  title: string;
  /** 1-2 sentence explanation of what happened, in plain language. */
  friendly_message: string;
  /** One concrete action the operator can take right now. */
  suggested_action: string;
  /** Optional deep link to docs. */
  learn_more_url?: string;
  /** The matched pattern key (for telemetry). Undefined when fallback is needed. */
  code?: string;
}

type Pattern = {
  code: string;
  regex: RegExp;
  title: string;
  friendly_message: string;
  suggested_action: string;
  learn_more_url?: string;
};

const DOCS_BASE = "https://github.com/nidal-z/apollia-os/wiki";

const PATTERNS: Pattern[] = [
  {
    code: "POLICY_DENIED",
    regex: /permission denied by policy|policy denied|policy violation/i,
    title: "Blocked by policy",
    friendly_message:
      "Your Apollia policy refuses this action for this agent or tool.",
    suggested_action:
      "Review the matching rule in Settings → Permissions, or create an exception.",
    learn_more_url: `${DOCS_BASE}/Permissions-Policy`,
  },
  {
    code: "EACCES",
    regex: /\bEACCES\b|permission denied/i,
    title: "Permission denied",
    friendly_message:
      "The system blocked this action because the current process doesn't have access to the target resource.",
    suggested_action:
      "Check that the path exists and that Apollia has read/write rights on it.",
    learn_more_url: `${DOCS_BASE}/Permissions-Filesystem`,
  },
  {
    code: "EPERM",
    regex: /\bEPERM\b|operation not permitted/i,
    title: "Operation not permitted",
    friendly_message:
      "The operating system refused this call, typically because of file flags, immutability, or sandbox policy.",
    suggested_action:
      "Try running the action on a file you own, or lift the system-level protection.",
    learn_more_url: `${DOCS_BASE}/Permissions-Filesystem`,
  },
  {
    code: "ENOENT",
    regex: /\bENOENT\b|no such file or directory/i,
    title: "File not found",
    friendly_message:
      "The requested path does not exist on disk.",
    suggested_action:
      "Verify the path spelling, or ask the agent to create it first.",
  },
  {
    code: "EISDIR",
    regex: /\bEISDIR\b|is a directory/i,
    title: "Path is a directory",
    friendly_message:
      "The action expected a file but received a directory path.",
    suggested_action: "Point the agent at a file inside the directory.",
  },
  {
    code: "ENOSPC",
    regex: /\bENOSPC\b|no space left on device/i,
    title: "No disk space",
    friendly_message: "The target volume is full.",
    suggested_action: "Free up disk space, then retry the action.",
  },
  {
    code: "TIMEOUT",
    regex: /\b(timeout|timed out|etimedout)\b/i,
    title: "Action timed out",
    friendly_message:
      "The operation did not complete within the allowed time window.",
    suggested_action:
      "Retry, or raise the timeout budget for this tool in Settings.",
  },
  {
    code: "NETWORK",
    regex: /\b(econnrefused|enetunreach|enotfound|dns)\b/i,
    title: "Network unreachable",
    friendly_message:
      "Apollia could not reach the remote endpoint — the host is down, offline, or unresolvable.",
    suggested_action: "Check your connection and the endpoint URL, then retry.",
  },
  {
    code: "BUDGET_EXCEEDED",
    regex: /budget exceeded|stepbudget|step budget/i,
    title: "Agent budget exhausted",
    friendly_message:
      "This agent has reached its StepBudget and cannot take more actions in this run.",
    suggested_action:
      "Raise the budget in the agent manifest, or start a fresh run.",
    learn_more_url: `${DOCS_BASE}/StepBudget`,
  },
  {
    code: "SANDBOX_VIOLATION",
    regex: /sandbox (violation|denied|blocked)/i,
    title: "Blocked by sandbox",
    friendly_message:
      "The tool tried to escape its sandbox and was stopped by the runtime.",
    suggested_action:
      "This is almost always intentional — review what the agent tried to do before granting access.",
    learn_more_url: `${DOCS_BASE}/Sandbox`,
  },
];

/**
 * Map a raw backend error message to a user-facing payload.
 *
 * Returns `undefined` when no pattern matches — callers should then fall
 * back to the Meta-LLM humanizer.
 */
export function permissionErrorHumanize(raw: string | null | undefined): HumanizedError | undefined {
  if (!raw || typeof raw !== "string") return undefined;
  for (const p of PATTERNS) {
    if (p.regex.test(raw)) {
      return {
        code: p.code,
        title: p.title,
        friendly_message: p.friendly_message,
        suggested_action: p.suggested_action,
        learn_more_url: p.learn_more_url,
      };
    }
  }
  return undefined;
}

/** Exposed for tests. */
export const _PATTERNS = PATTERNS;
