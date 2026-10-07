/**
 * The verdict of "Verify integrity": what the report of the whole journal lets
 * the screen say, and nothing beyond it.
 *
 * The verification proves that the entries present follow one another as they
 * were written. It does not prove that none was removed at the end: only an
 * anchor kept elsewhere does, and every verdict says so.
 */
import type { VerifyJournalReport } from "$lib/ipc/journal";

/** Where the verified part of the journal begins. */
export type VerifiedSince =
  /** At the first entry ever written. */
  | { from: "origin" }
  /** At the first entry a purge by retention kept, written on `firstEntryAt`. */
  | { from: "purge"; firstEntryAt: string };

export type Verdict =
  /** Nothing to verify yet. */
  | { kind: "empty" }
  /** Every entry present follows the one before it. */
  | { kind: "intact"; entries: number; since: VerifiedSince }
  /**
   * The chain breaks at the entry `entry`. `intactBefore` counts the entries
   * that precede it and still hold, `null` when the report does not prove it.
   */
  | { kind: "altered"; entry: number; intactBefore: number | null; since: VerifiedSince }
  /** The entries follow one another, but the last is not the recorded end. */
  | { kind: "end_mismatch"; entries: number; since: VerifiedSince };

function sinceOf(report: VerifyJournalReport): VerifiedSince {
  const start = report.start ?? null;
  return start === null ? { from: "origin" } : { from: "purge", firstEntryAt: start.ts };
}

/**
 * The verdict a report supports. A break found inside one run is reported
 * without a count of intact entries: the runs are walked one after the other,
 * so the entry named is a broken one, not necessarily the earliest.
 */
export function verdictOf(report: VerifyJournalReport): Verdict {
  const since = sinceOf(report);
  const broken = report.first_break;
  if (broken !== null) {
    const first = report.start?.global_seq ?? 0;
    const intactBefore =
      broken.reason === "per_run_broken" ? null : Math.max(0, broken.global_seq - first);
    return { kind: "altered", entry: broken.global_seq, intactBefore, since };
  }
  if (!report.ok || !report.head_matches_state) {
    return { kind: "end_mismatch", entries: report.entries_checked, since };
  }
  if (report.entries_checked === 0) return { kind: "empty" };
  return { kind: "intact", entries: report.entries_checked, since };
}

/** Whether a verdict is one the journal can still be handed over under. */
export function isSound(verdict: Verdict): boolean {
  return verdict.kind === "intact" || verdict.kind === "empty";
}

/** Where a verification stands. */
export type VerifyState =
  | { status: "idle" }
  | { status: "running" }
  | { status: "done"; verdict: Verdict; checkedAt: Date }
  | { status: "failed"; message: string };
