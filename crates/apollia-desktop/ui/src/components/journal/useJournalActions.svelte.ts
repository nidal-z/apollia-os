/**
 * The live state of the two actions of the journal header: the verification
 * of the whole chain, and the export, as a CSV file or as a page to print.
 */
import {
  exportJournal,
  printJournal,
  readJournalExport,
  verifyJournal,
} from "$lib/ipc/journal";
import type {
  JournalExportDocument,
  JournalExported,
  JournalQuery,
  VerifyJournalReport,
} from "$lib/ipc/journal";
import { getOrgPolicy, type PolicyView } from "$lib/ipc/policy";
import { detailFor, detailRule, type DetailRule, type ExportFormat } from "./journalExport";
import { verdictOf, type VerifyState } from "./journalVerify";

/** The shell calls the two actions make, replaceable in tests. */
export interface JournalActionsIpc {
  verify: () => Promise<VerifyJournalReport>;
  policy: () => Promise<PolicyView>;
  exportTo: (query: JournalQuery, detail: boolean, path: string) => Promise<JournalExported>;
  readExport: (query: JournalQuery, detail: boolean) => Promise<JournalExportDocument>;
  print: () => Promise<void>;
}

export const shellActionsIpc: JournalActionsIpc = {
  verify: verifyJournal,
  policy: getOrgPolicy,
  exportTo: exportJournal,
  readExport: readJournalExport,
  print: printJournal,
};

/** What an export is asked with. */
export interface ExportRequest {
  format: ExportFormat;
  /** What the user ticked; the policy may settle it otherwise. */
  wantsDetail: boolean;
  query: JournalQuery;
  /** Asks where the CSV file goes; `null` when the user gave up. */
  pickPath: () => Promise<string | null>;
}

/** How an export ended. */
export type ExportOutcome =
  | { status: "written"; path: string }
  /** The page to print is laid out, waiting for the print dialog. */
  | { status: "preview" }
  | { status: "cancelled" }
  | { status: "failed"; message: string };

export interface JournalActions {
  readonly verification: VerifyState;
  /** Whether the export dialog is shown. */
  readonly exportOpen: boolean;
  /** What the policy settles about the detail, read when the dialog opens. */
  readonly detailRule: DetailRule;
  readonly exporting: boolean;
  /** Why the last export asked could not be made, `null` when none failed. */
  readonly exportError: string | null;
  /** The export laid out as a page to print, `null` when none is shown. */
  readonly printed: JournalExportDocument | null;
  verify(now?: Date): Promise<void>;
  dismissVerdict(): void;
  openExport(): Promise<void>;
  closeExport(): void;
  runExport(request: ExportRequest): Promise<ExportOutcome>;
  print(): Promise<string | null>;
  closePrint(): void;
}

function messageOf(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

export function createJournalActions(ipc: JournalActionsIpc = shellActionsIpc): JournalActions {
  let verification = $state<VerifyState>({ status: "idle" });
  let exportOpen = $state(false);
  let rule = $state<DetailRule>("free");
  let exporting = $state(false);
  let exportError = $state<string | null>(null);
  let printed = $state<JournalExportDocument | null>(null);

  async function verify(now: Date = new Date()): Promise<void> {
    if (verification.status === "running") return;
    verification = { status: "running" };
    try {
      verification = { status: "done", verdict: verdictOf(await ipc.verify()), checkedAt: now };
    } catch (err) {
      verification = { status: "failed", message: messageOf(err) };
    }
  }

  async function openExport(): Promise<void> {
    rule = "free";
    exportError = null;
    exportOpen = true;
    try {
      rule = detailRule(await ipc.policy());
    } catch {
      // Unread, the policy settles nothing here; the runtime still applies it.
      rule = "free";
    }
  }

  async function runExport(request: ExportRequest): Promise<ExportOutcome> {
    if (exporting) return { status: "cancelled" };
    const detail = detailFor(rule, request.wantsDetail);
    exporting = true;
    exportError = null;
    try {
      if (request.format === "pdf") {
        printed = await ipc.readExport(request.query, detail);
        exportOpen = false;
        return { status: "preview" };
      }
      const path = await request.pickPath();
      if (path === null) return { status: "cancelled" };
      const written = await ipc.exportTo(request.query, detail, path);
      exportOpen = false;
      return { status: "written", path: written.path };
    } catch (err) {
      exportError = messageOf(err);
      return { status: "failed", message: exportError };
    } finally {
      exporting = false;
    }
  }

  /** Opens the print dialog; answers why it could not, `null` when it did. */
  async function print(): Promise<string | null> {
    try {
      await ipc.print();
      return null;
    } catch (err) {
      return messageOf(err);
    }
  }

  return {
    get verification() { return verification; },
    get exportOpen() { return exportOpen; },
    get detailRule() { return rule; },
    get exporting() { return exporting; },
    get exportError() { return exportError; },
    get printed() { return printed; },
    verify,
    dismissVerdict: () => { verification = { status: "idle" }; },
    openExport,
    closeExport: () => { exportOpen = false; },
    runExport,
    print,
    closePrint: () => { printed = null; },
  };
}
