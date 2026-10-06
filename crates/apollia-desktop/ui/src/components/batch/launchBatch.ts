/**
 * The decisions of the launch dialog of a batch, kept out of the component so
 * they can be tested without rendering it: which sources a batch can read,
 * what a launch is refused for, and the body it sends.
 */
import type {
  BatchAccess,
  BatchLaunch,
  BatchSourceDescriptor,
  RegistrySource,
  WriteDestination,
} from "$lib/ipc/batches";

/** The failure counts the dialog offers, in order. */
export const FAILURE_THRESHOLDS = [5, 10, 20, 50] as const;

/** The threshold a launch starts from. */
export const DEFAULT_THRESHOLD = "10";

/** The choice that lets a batch run to its end whatever fails. */
export const NEVER_INTERRUPT = "never";

/**
 * What "never interrupt" sends: the runtime interrupts once the failures
 * exceed the count, and no batch exceeds the largest count it stores.
 */
export const UNBOUNDED_FAILURES = 4_294_967_295;

/** The access a launch grants when its results go into the original file. */
export const ORIGINAL_WRITE_GRANT = "source:write";

/** The most columns one launch adds to its file. */
export const MAX_COLUMNS = 32;

const WORKBOOK_EXTENSIONS = ["xlsx", "xlsm", "xlsb", "xls", "ods"];
const EDITABLE_EXTENSIONS = ["xlsx", "xlsm"];
const MAILBOX_PREFIXES = ["imap:", "outlook:"];

/** The extensions a file picker offers for a batch. */
export const TABLE_EXTENSIONS = [...WORKBOOK_EXTENSIONS, "csv"];

/** Why a listed source cannot be launched on. */
export type SourceBlock = "folder_unread" | "file_not_table" | "mailbox_unknown";

/** The three families of sources the dialog offers, in order. */
export const SOURCE_FAMILIES = ["table", "folder", "mail"] as const;

/** A family of sources: a workbook or CSV file, a folder, a mailbox. */
export type SourceFamily = (typeof SOURCE_FAMILIES)[number];

/** One source the dialog offers. */
export interface SourceChoice {
  /** Stable key of the option. */
  key: string;
  /** What the option shows as its icon and its kind. */
  shape: SourceFamily;
  label: string;
  /** The path or the mailbox, shown under the label. */
  detail: string;
  /** The reader and location of a launch; `null` when no batch reads it. */
  reader: { kind: string; location: string } | null;
  /** Why no batch reads it, when `reader` is `null`. */
  blocked: SourceBlock | null;
}

/** What the dialog holds when the user launches. */
export interface LaunchForm {
  agent: string | null;
  source: SourceChoice | null;
  /** The accesses checked, by their identifier. */
  granted: string[];
  /** A count of `FAILURE_THRESHOLDS` as text, or `NEVER_INTERRUPT`. */
  threshold: string;
  destination: WriteDestination;
  /** The columns added to the file, as typed, comma separated. */
  columns: string;
  /** A mailbox yields its unread messages only. */
  unreadOnly: boolean;
  /**
   * The sheet, the header row and the checked columns of a previewed file;
   * absent or `null` while no preview is known, and the runtime then reads
   * what it proposes.
   */
  table?: { options: Record<string, string>; checked: number } | null;
}

/** Why a launch is refused before it is sent. */
export type LaunchRefusal =
  | "agent_missing"
  | "source_missing"
  | "source_blocked"
  | "fields_missing"
  | "columns_missing"
  | "columns_duplicate"
  | "columns_too_many";

function extensionOf(path: string): string {
  const name = path.slice(Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\")) + 1);
  const dot = name.lastIndexOf(".");
  return dot > 0 ? name.slice(dot + 1).toLowerCase() : "";
}

/** The last segment of `path`. */
export function fileNameOf(path: string): string {
  return path.slice(Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\")) + 1);
}

/** The reader of the file at `path`: `spreadsheet`, `csv`, or none. */
export function tableReaderOf(path: string): "spreadsheet" | "csv" | null {
  const extension = extensionOf(path);
  if (extension === "csv") return "csv";
  return WORKBOOK_EXTENSIONS.includes(extension) ? "spreadsheet" : null;
}

/** Whether the results may go into the file at `location` itself. */
export function canWriteOriginal(source: SourceChoice | null): boolean {
  return (
    source?.reader?.kind === "spreadsheet" &&
    EDITABLE_EXTENSIONS.includes(extensionOf(source.reader.location))
  );
}

/** Whether a batch on `source` writes its results into a file. */
export function writesResults(source: SourceChoice | null): boolean {
  return source?.shape === "table" && source.reader !== null;
}

/** The option of a file chosen by its path: dropped in the chat, or picked. */
export function fileChoice(path: string): SourceChoice {
  const kind = tableReaderOf(path);
  return {
    key: `path:${path}`,
    shape: "table",
    label: fileNameOf(path),
    detail: path,
    reader: kind ? { kind, location: path } : null,
    blocked: kind ? null : "file_not_table",
  };
}

function registryChoice(source: RegistrySource): SourceChoice | null {
  const reference = source.reference ?? "";
  const base = { key: `source:${source.id}`, label: source.label, detail: reference };
  if (source.kind === "file") {
    const kind = tableReaderOf(reference);
    return {
      ...base,
      shape: "table",
      reader: kind ? { kind, location: reference } : null,
      blocked: kind ? null : "file_not_table",
    };
  }
  if (source.kind === "mail") {
    const known = MAILBOX_PREFIXES.some((prefix) => reference.startsWith(prefix));
    return {
      ...base,
      shape: "mail",
      reader: known ? { kind: "mail", location: reference } : null,
      blocked: known ? null : "mailbox_unknown",
    };
  }
  if (source.kind === "folder") {
    return { ...base, shape: "folder", reader: null, blocked: "folder_unread" };
  }
  return null;
}

/**
 * The sources the dialog offers: the file it was opened on first, then the
 * files, folders and mailboxes of the registry. The profile and the memory
 * hold no items to run an agent on, and are left out.
 */
export function sourceChoices(registry: RegistrySource[], file: string | null): SourceChoice[] {
  const choices: SourceChoice[] = file ? [fileChoice(file)] : [];
  for (const source of registry) {
    const choice = registryChoice(source);
    if (choice === null) continue;
    if (choice.reader && choices.some((c) => c.reader?.location === choice.reader?.location)) continue;
    choices.push(choice);
  }
  return choices;
}

/** The sources of `family` among `choices`, in order. */
export function familyChoices(choices: SourceChoice[], family: SourceFamily): SourceChoice[] {
  return choices.filter((choice) => choice.shape === family);
}

/** The columns typed, trimmed, in order, blanks dropped. */
export function columnsOf(typed: string): string[] {
  return typed
    .split(",")
    .map((column) => column.trim())
    .filter((column) => column !== "");
}

/** The failures a launch tolerates for the choice `threshold`. */
export function maxFailuresOf(threshold: string): number {
  if (threshold === NEVER_INTERRUPT) return UNBOUNDED_FAILURES;
  const count = Number.parseInt(threshold, 10);
  return Number.isInteger(count) && count >= 0 ? count : Number.parseInt(DEFAULT_THRESHOLD, 10);
}

/** The accesses a launch may grant, and those an item will wait on. */
export function splitAccesses(accesses: BatchAccess[]): {
  grantable: BatchAccess[];
  asked: BatchAccess[];
} {
  return {
    grantable: accesses.filter((access) => access.grantable),
    asked: accesses.filter((access) => !access.grantable),
  };
}

/** Why `form` cannot be launched, or `null` when it can. */
export function refusalOf(form: LaunchForm): LaunchRefusal | null {
  if (!form.agent) return "agent_missing";
  if (form.source === null) return "source_missing";
  if (form.source.reader === null) return "source_blocked";
  if (writesResults(form.source)) {
    if (form.table && form.table.checked === 0) return "fields_missing";
    const columns = columnsOf(form.columns);
    if (columns.length === 0) return "columns_missing";
    if (columns.length > MAX_COLUMNS) return "columns_too_many";
    const seen = new Set(columns.map((column) => column.toLowerCase()));
    if (seen.size !== columns.length) return "columns_duplicate";
  }
  return null;
}

/**
 * The body of the launch of `form`, or `null` when it is refused. The
 * original file is written only when the form asks for it and the file
 * allows it, and the launch then carries the access that covers that write.
 */
export function buildLaunch(form: LaunchForm, accesses: BatchAccess[]): BatchLaunch | null {
  if (refusalOf(form) !== null || !form.agent || !form.source?.reader) return null;
  const grantable = new Set(splitAccesses(accesses).grantable.map((access) => access.accessId));
  const grants = form.granted.filter((id) => grantable.has(id));
  const source: BatchSourceDescriptor = { ...form.source.reader, options: {} };
  if (writesResults(form.source)) {
    if (form.table) source.options = { ...form.table.options };
    const original = form.destination === "original" && canWriteOriginal(form.source);
    source.write = {
      destination: original ? "original" : "copy",
      columns: columnsOf(form.columns),
    };
    if (original) grants.push(ORIGINAL_WRITE_GRANT);
  }
  if (form.source.shape === "mail" && form.unreadOnly) source.options.unread_only = "true";
  return {
    agent: form.agent,
    source,
    grants,
    maxFailures: maxFailuresOf(form.threshold),
  };
}

/**
 * The path a batch can be launched on for a file joined to the chat: a
 * workbook or a CSV file whose path is known, which a dropped file is.
 */
export function batchFileOf(attachment: { name: string; absolutePath?: string }): string | null {
  const path = attachment.absolutePath;
  if (!path || tableReaderOf(attachment.name) === null) return null;
  return tableReaderOf(path) === null ? null : path;
}

/** The catalogue keys of a family of sources: its name, what becomes an item, its empty list. */
export const FAMILY_KEYS: Record<SourceFamily, { name: string; item: string; none: string }> = {
  table: {
    name: "batch_launch.family.table.name",
    item: "batch_launch.family.table.item",
    none: "batch_launch.family.table.none",
  },
  folder: {
    name: "batch_launch.family.folder.name",
    item: "batch_launch.family.folder.item",
    none: "batch_launch.family.folder.none",
  },
  mail: {
    name: "batch_launch.family.mail.name",
    item: "batch_launch.family.mail.item",
    none: "batch_launch.family.mail.none",
  },
};

/** The catalogue key of the reason a listed source cannot be launched on. */
export const BLOCK_KEYS: Record<SourceBlock, string> = {
  folder_unread: "batch_launch.source.blocked.folder_unread",
  file_not_table: "batch_launch.source.blocked.file_not_table",
  mailbox_unknown: "batch_launch.source.blocked.mailbox_unknown",
};

/** The catalogue key of the sentence a refused launch shows. */
export const REFUSAL_KEYS: Record<LaunchRefusal, string> = {
  agent_missing: "batch_launch.refusal.agent_missing",
  source_missing: "batch_launch.refusal.source_missing",
  source_blocked: "batch_launch.refusal.source_blocked",
  fields_missing: "batch_launch.refusal.fields_missing",
  columns_missing: "batch_launch.refusal.columns_missing",
  columns_duplicate: "batch_launch.refusal.columns_duplicate",
  columns_too_many: "batch_launch.refusal.columns_too_many",
};

const ACCESS_FAMILY_KEYS: Record<string, string> = {
  read_personal: "batch_launch.accesses.family.read_personal",
  write_or_delete: "batch_launch.accesses.family.write_or_delete",
  send_external: "batch_launch.accesses.family.send_external",
  execute_code: "batch_launch.accesses.family.execute_code",
};

/** The catalogue key of the action family of an access; an unknown one reads as unclassified. */
export function accessFamilyKey(family: string): string {
  return ACCESS_FAMILY_KEYS[family] ?? "batch_launch.accesses.family.unclassified";
}
