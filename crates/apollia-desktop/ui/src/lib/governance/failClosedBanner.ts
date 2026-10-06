/**
 * What the fail-closed banner says, decided from the state of governance.
 *
 * A plain module so the decision is tested without mounting the banner.
 */
import type { FailClosed } from "$lib/ipc/governance";

/** The key naming each policy refusal class in words. */
const POLICY_REASON_KEYS: Readonly<Record<string, string>> = {
  schema: "governance.fail_closed.policy_reason.schema",
  signature: "governance.fail_closed.policy_reason.signature",
  key: "governance.fail_closed.policy_reason.key",
  owner: "governance.fail_closed.policy_reason.owner",
};

/** The key of a refusal class this interface does not know. */
const UNKNOWN_POLICY_REASON_KEY = "governance.fail_closed.policy_reason.unknown";

/** What the banner offers: read the database again, or open the policy. */
export type FailClosedAction = "retry" | "view_policy";

/** The banner of one fail-closed state, as i18n keys. */
export interface FailClosedBanner {
  /** Key of the sentence that says why. */
  reasonKey: string;
  /** Key of the refusal class the policy sentence names, `null` otherwise. */
  policyReasonKey: string | null;
  /** The action at the right of the banner. */
  action: FailClosedAction;
}

/**
 * The banner for `failClosed`, or `null` while governance decides normally.
 *
 * An unreadable database can be repaired in place, so the banner offers to
 * read it again. A refused policy is read once at start and only the IT
 * department corrects it, so the banner opens the policy instead. A journal
 * the policy requires and that is not open is the same case: the policy says
 * what is required, and the journal is opened at start.
 */
export function failClosedBanner(failClosed: FailClosed | null): FailClosedBanner | null {
  if (failClosed === null) return null;
  if (failClosed.cause === "store_unreadable") {
    return {
      reasonKey: "governance.fail_closed.store_unreadable",
      policyReasonKey: null,
      action: "retry",
    };
  }
  if (failClosed.cause === "journal_unavailable") {
    return {
      reasonKey: "governance.fail_closed.journal_unavailable",
      policyReasonKey: null,
      action: "view_policy",
    };
  }
  return {
    reasonKey: "governance.fail_closed.policy_invalid",
    policyReasonKey: Object.hasOwn(POLICY_REASON_KEYS, failClosed.reason)
      ? POLICY_REASON_KEYS[failClosed.reason]
      : UNKNOWN_POLICY_REASON_KEY,
    action: "view_policy",
  };
}
