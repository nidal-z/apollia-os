/**
 * The notice of the migration of the former projects into sources: what the
 * report of the runtime says, shaped for the dialog, and whether the dialog
 * has already been shown.
 *
 * The report is the one `GET /api/v1/sources/project-migration` answers; the
 * desktop reads it through `get_project_migration_report`. The notice is shown
 * once per outcome: once when a run was partial, once more when a later run
 * completes, never again after that.
 */

/** Where a run of the migration ended. */
export type MigrationStatus = "complete" | "partial";

/** What an issue of the report is about. */
export type IssueItem =
  | "store"
  | "document"
  | "instructions"
  | "agent"
  | "folder"
  | "provider"
  | "memory"
  | "conversations";

/** Why an item was not carried over. */
export type IssueReason =
  | "missing"
  | "unreadable"
  | "wrong_kind"
  | "agent_not_installed"
  | "failed"
  | "not_carried_over";

/** One item the migration did not carry over. */
export interface MigrationIssue {
  projectId?: string | null;
  projectName?: string | null;
  item: IssueItem;
  subject: string;
  reason: IssueReason;
  detail?: string | null;
  blocking: boolean;
}

/** What the migration did for one project. */
export interface ProjectOutcome {
  id: string;
  name: string;
  documentSources: string[];
  instructionsSource?: string | null;
  agentsChecked: string[];
  folderConnection?: string | null;
  codeFolderSource?: string | null;
}

/** The report of a run of the migration. */
export interface MigrationReport {
  formatVersion: number;
  status: MigrationStatus;
  ranAt: string;
  projects: ProjectOutcome[];
  conversationsDetached: number;
  issues: MigrationIssue[];
}

/**
 * The three shapes of the notice: everything that could be carried over was,
 * a run stopped before the end, or nothing could be read at all.
 */
export type NoticeKind = "complete" | "partial" | "failed";

/** The notice, as the dialog renders it. */
export interface NoticeModel {
  kind: NoticeKind;
  projects: number;
  documents: number;
  instructions: number;
  folders: number;
  /** What was not carried over, in the order of the report. */
  notCarried: MigrationIssue[];
}

/** Local storage key of the outcomes already shown. */
export const NOTICE_SEEN_KEY = "apollia.projectMigration.noticeSeen";

/** The notice of `report`. */
export function noticeModel(report: MigrationReport): NoticeModel {
  const projects = report.projects ?? [];
  const kind: NoticeKind =
    report.status === "complete"
      ? "complete"
      : projects.length === 0
        ? "failed"
        : "partial";
  return {
    kind,
    projects: projects.length,
    documents: projects.reduce((n, p) => n + (p.documentSources?.length ?? 0), 0),
    instructions: projects.filter((p) => !!p.instructionsSource).length,
    folders: projects.filter((p) => !!p.folderConnection).length,
    notCarried: kind === "failed" ? [] : [...(report.issues ?? [])],
  };
}

/** The mark that says the notice of `report` was shown. */
export function seenMark(report: MigrationReport): string {
  return report.status;
}

/**
 * Whether the notice of `report` is still to be shown, given the marks
 * already recorded. No report, nothing to tell.
 */
export function shouldShow(
  report: MigrationReport | null,
  seen: readonly string[],
): boolean {
  if (!report) return false;
  return !seen.includes(seenMark(report));
}

/** The marks recorded in this browser, empty when there are none. */
export function readSeen(storage: Pick<Storage, "getItem"> | undefined): string[] {
  if (!storage) return [];
  try {
    const raw = storage.getItem(NOTICE_SEEN_KEY);
    const parsed: unknown = raw ? JSON.parse(raw) : [];
    return Array.isArray(parsed) ? parsed.filter((v): v is string => typeof v === "string") : [];
  } catch {
    return [];
  }
}

/** Records that the notice of `report` was shown. */
export function recordSeen(
  storage: Pick<Storage, "getItem" | "setItem"> | undefined,
  report: MigrationReport,
): void {
  if (!storage) return;
  const seen = readSeen(storage);
  const mark = seenMark(report);
  if (seen.includes(mark)) return;
  try {
    storage.setItem(NOTICE_SEEN_KEY, JSON.stringify([...seen, mark]));
  } catch {
    // Quota or private mode: the notice may show again, nothing is lost.
  }
}
