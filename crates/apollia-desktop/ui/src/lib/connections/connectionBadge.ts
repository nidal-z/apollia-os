/**
 * The state badge of a connection: one of the four states the runtime finds
 * (working, needs repair, paused, blocked), why it needs repair, and how long
 * ago it was found.
 *
 * Pure and i18n-agnostic: it returns keys and values, the component resolves
 * them with the live formatter.
 */
import type { I18nDescriptor } from "$lib/connections/status";
import type { ConnectionReason, ConnectionReasonKey, ConnectionStatusKey } from "$lib/types";

/** Badge variant of a state (mirrors the `Badge` variants). */
export type StateBadgeVariant = "success" | "warning" | "neutral" | "danger";

/** What the badge of one connection says. */
export interface StateBadge {
  variant: StateBadgeVariant;
  /** Key of the word of the state. */
  labelKey: string;
  /** Key of why it needs repair; null in any other state. */
  reasonKey: string | null;
  /** How long ago the state was found; null when it never was. */
  when: I18nDescriptor | null;
}

const LABEL_KEY: Record<ConnectionStatusKey, string> = {
  working: "connections.state.working",
  needs_repair: "connections.state.needs_repair",
  paused: "connections.state.paused",
  blocked: "connections.state.blocked",
};

const REASON_KEY: Record<ConnectionReasonKey, string> = {
  not_checked: "connections.state.reason.not_checked",
  sign_in_again: "connections.state.reason.sign_in_again",
  unreachable: "connections.state.reason.unreachable",
  failing: "connections.state.reason.failing",
  forbidden_by_policy: "connections.state.reason.forbidden_by_policy",
};

const VARIANT: Record<ConnectionStatusKey, StateBadgeVariant> = {
  working: "success",
  needs_repair: "warning",
  paused: "neutral",
  blocked: "danger",
};

/** How long ago `checkedAt` was, at `now`; null when absent, unreadable or ahead of `now`. */
export function checkedDescriptor(checkedAt: string | null, now: number): I18nDescriptor | null {
  if (!checkedAt) return null;
  const diffMs = now - new Date(checkedAt).getTime();
  if (Number.isNaN(diffMs) || diffMs < 0) return null;
  const mins = Math.floor(diffMs / 60_000);
  if (mins < 1) return { key: "connections.state.checked_just_now" };
  if (mins < 60) return { key: "connections.state.checked_minutes_ago", values: { mins } };
  const hours = Math.floor(mins / 60);
  if (hours < 24) return { key: "connections.state.checked_hours_ago", values: { hours } };
  return { key: "connections.state.checked_days_ago", values: { days: Math.floor(hours / 24) } };
}

/** The badge of a connection in `status`, for `reason`, found at `checkedAt`. */
export function stateBadge(
  status: ConnectionStatusKey,
  reason: ConnectionReason | null,
  checkedAt: string | null,
  now: number = Date.now(),
): StateBadge {
  return {
    variant: VARIANT[status],
    labelKey: LABEL_KEY[status],
    reasonKey: status === "needs_repair" && reason ? REASON_KEY[reason.key] : null,
    when: checkedDescriptor(checkedAt, now),
  };
}
