/**
 * The decisions behind the "Sources" section of an agent's sheet, kept apart
 * from the markup so they are tested on their own (Vitest runs in `node`).
 *
 * The section lists the sources of the registry kind by kind, each checked
 * or not for the agent, with where its index stands: being read, ready,
 * failed, or searched by keywords only while no model for searching by
 * meaning is installed. The profile and the agent's memory point at nothing
 * and are named by their token. A source is added from a folder connection,
 * a file of this machine or a mailbox connection.
 *
 * Checking a source lets the agent search it; it grants no tool. A folder
 * source whose connection the agent cannot read says so, and offers the
 * reading tools of that connection.
 */
import type { IndexState, NewSource, Source, SourceIndex, SourceKind } from "$lib/ipc/sources";
import type { Connection } from "$lib/types";

/** The kinds, in the order the section lists them. */
export const SOURCE_KIND_ORDER: readonly SourceKind[] = [
  "folder",
  "file",
  "mail",
  "code_folder",
  "profile",
  "memory",
];

/** The two sources named by their token, always offered. */
export const TOKEN_SOURCES: readonly SourceKind[] = ["profile", "memory"];

/** What a row says about its source. */
export type SourceRowState =
  /** Waiting to be read. */
  | "pending"
  | "indexing"
  | "ready"
  | "error"
  | "keywords_only"
  /** Nothing to index: the profile, the memory. */
  | "none"
  /** Named by the agent, absent from the registry. */
  | "missing";

/** One row of the section. */
export interface SourceRow {
  /** What the agent's `sources` list names. */
  id: string;
  kind: SourceKind | "missing";
  /** The name stored in the registry; empty for the profile and the memory. */
  label: string;
  checked: boolean;
  state: SourceRowState;
  /** Files read so far. */
  documents: number;
  /** Files left out, over 50 MB. */
  tooLarge: number;
  /** Files left out, that no reader could open. */
  unreadable: number;
  /** Paths of the files left out. */
  skipped: { path: string; reason: "too_large" | "unreadable" }[];
  /** The folder connection or mailbox it points at. */
  reference: string | null;
  /** Whether it is a row of the registry, which can be removed. */
  registered: boolean;
}

const STATE_OF: Record<IndexState, SourceRowState> = {
  absent: "pending",
  indexing: "indexing",
  ready: "ready",
  error: "error",
  keywords_only: "keywords_only",
};

function tokenRow(kind: SourceKind, checked: boolean): SourceRow {
  return {
    id: kind,
    kind,
    label: "",
    checked,
    state: "none",
    documents: 0,
    tooLarge: 0,
    unreadable: 0,
    skipped: [],
    reference: null,
    registered: false,
  };
}

function registryRow(source: Source, index: SourceIndex | undefined, checked: boolean): SourceRow {
  const skipped = index?.skipped ?? [];
  return {
    id: source.id,
    kind: source.kind,
    label: source.label,
    checked,
    state: STATE_OF[index?.indexState ?? source.indexState],
    documents: index?.documents ?? 0,
    tooLarge: skipped.filter((file) => file.reason === "too_large").length,
    unreadable: skipped.filter((file) => file.reason === "unreadable").length,
    skipped: skipped.map((file) => ({ path: file.path, reason: file.reason })),
    reference: source.reference,
    registered: true,
  };
}

/**
 * The rows of the section. With `all`, every source of the registry and the
 * two token sources, checked or not (the adjustment); without, only what the
 * agent consults (the reading). An entry the registry no longer holds is
 * kept, marked missing, so it can be unchecked.
 */
export function sourceRows(
  registry: Source[],
  indexes: Record<string, SourceIndex>,
  checked: string[],
  all: boolean,
): SourceRow[] {
  const on = new Set(checked);
  const rows: SourceRow[] = [];
  const known = new Set<string>(TOKEN_SOURCES);
  for (const source of registry) {
    // The profile and the memory are named by their token, never by a row.
    if (TOKEN_SOURCES.includes(source.kind)) continue;
    known.add(source.id);
    if (all || on.has(source.id)) rows.push(registryRow(source, indexes[source.id], on.has(source.id)));
  }
  for (const kind of TOKEN_SOURCES) {
    if (all || on.has(kind)) rows.push(tokenRow(kind, on.has(kind)));
  }
  const order = (row: SourceRow): number =>
    row.kind === "missing" ? SOURCE_KIND_ORDER.length : SOURCE_KIND_ORDER.indexOf(row.kind);
  rows.sort((a, b) => order(a) - order(b));
  for (const id of checked) {
    if (known.has(id)) continue;
    rows.push({ ...tokenRow("profile", true), id, kind: "missing", label: id, state: "missing" });
  }
  return rows;
}

/** Catalogue key of the name of a row, which takes the stored `name`. */
export const SOURCE_NAME_KEY: Record<SourceRow["kind"], string | null> = {
  folder: "agent_sources.name_folder",
  file: "agent_sources.name_file",
  mail: "agent_sources.name_mail",
  code_folder: "agent_sources.name_code_folder",
  profile: "agent_sheet.source_profile",
  memory: "agent_sheet.source_memory",
  missing: null,
};

/** Catalogue key of the line under the name, which takes the files read as `count`. */
export function sourceStatusKey(row: SourceRow): string {
  if (row.kind === "profile") return "agent_sources.about_profile";
  if (row.kind === "memory") return "agent_sources.about_memory";
  switch (row.state) {
    case "indexing":
      return "agent_sources.status_indexing";
    case "ready":
      return "agent_sources.status_ready";
    case "keywords_only":
      return "agent_sources.status_keywords_only";
    case "error":
      return "agent_sources.status_error";
    case "missing":
      return "agent_sources.status_missing";
    default:
      return "agent_sources.status_pending";
  }
}

/** Catalogue key and tone of the state badge; `null` when the row has none. */
export const SOURCE_STATE_BADGE: Record<
  SourceRowState,
  { label: string; tone: "success" | "info" | "danger" | "warning" | "neutral" } | null
> = {
  ready: { label: "agent_sources.state_ready", tone: "success" },
  indexing: { label: "agent_sources.state_indexing", tone: "info" },
  error: { label: "agent_sources.state_error", tone: "danger" },
  keywords_only: { label: "agent_sources.state_keywords_only", tone: "warning" },
  pending: { label: "agent_sources.state_pending", tone: "neutral" },
  missing: { label: "agent_sources.state_missing", tone: "danger" },
  none: null,
};

/** Catalogue key of why a file was left out. */
export const SKIP_REASON_KEY: Record<"too_large" | "unreadable", string> = {
  too_large: "agent_sources.skipped_too_large",
  unreadable: "agent_sources.skipped_unreadable",
};

/** Whether a row is still being read, so the section asks again shortly. */
export function awaitsIndex(rows: SourceRow[]): boolean {
  return rows.some((row) => row.kind !== "mail" && (row.state === "pending" || row.state === "indexing"));
}

/** The sources whose index is read: every one that points at documents. */
export function indexedSources(registry: Source[]): Source[] {
  return registry.filter((source) => source.kind === "folder" || source.kind === "file" || source.kind === "code_folder");
}

// ── Adding ──────────────────────────────────────────────────────────────

/** The kinds the "Add a source" menu offers, the code folder in developer mode only. */
export function addChoices(folderKinds: SourceKind[]): SourceKind[] {
  const kinds: SourceKind[] = ["folder", "file", "mail", "profile", "memory"];
  if (folderKinds.includes("code_folder")) kinds.push("code_folder");
  return kinds;
}

/** One connection a source can be made on. */
export interface SourceCandidate {
  /** What the source will point at. */
  reference: string;
  label: string;
  /** The folder path or the account, shown under the name. */
  detail: string;
}

/** The folder connections a folder or a code folder source is chosen among. */
export function folderCandidates(connections: Connection[]): SourceCandidate[] {
  return connections
    .filter((connection) => connection.kind === "folder")
    .map((connection) => ({
      reference: connection.id,
      label: connection.label,
      detail: connection.path ?? "",
    }));
}

const MICROSOFT_PREFIX = "native:microsoft:";

/**
 * The mailbox connections a mail source is chosen among: an IMAP mailbox
 * (`imap:<address>`) and an Outlook account (`outlook:<account>`), the two
 * places a mailbox is read from.
 */
export function mailCandidates(connections: Connection[]): SourceCandidate[] {
  const candidates: SourceCandidate[] = [];
  for (const connection of connections) {
    if (connection.kind === "imap") {
      candidates.push({ reference: connection.id, label: connection.label, detail: connection.account ?? "" });
    } else if (connection.kind === "native" && connection.id.startsWith(MICROSOFT_PREFIX)) {
      const account = connection.id.slice(MICROSOFT_PREFIX.length);
      candidates.push({ reference: `outlook:${account}`, label: connection.label, detail: connection.account ?? account });
    }
  }
  return candidates;
}

/** The name of a file, without the folders above it. */
export function fileName(path: string): string {
  const parts = path.split(/[\\/]/).filter((part) => part.length > 0);
  return parts[parts.length - 1] ?? path;
}

/** Title and line of each entry of the "Add a source" menu. */
export const ADD_CHOICE_KEYS: Record<SourceKind, { title: string; hint: string }> = {
  folder: { title: "agent_sources.add_folder", hint: "agent_sources.add_folder_hint" },
  file: { title: "agent_sources.add_file", hint: "agent_sources.add_file_hint" },
  mail: { title: "agent_sources.add_mail", hint: "agent_sources.add_mail_hint" },
  profile: { title: "agent_sources.add_profile", hint: "agent_sources.about_profile" },
  memory: { title: "agent_sources.add_memory", hint: "agent_sources.about_memory" },
  code_folder: { title: "dev.agent_sources.add_code_folder", hint: "dev.agent_sources.add_code_folder_hint" },
};

/** The source of the registry that already is `wanted`, so it is checked rather than added twice. */
export function existingSource(registry: Source[], wanted: NewSource): Source | null {
  return registry.find((source) => source.kind === wanted.kind && source.reference === wanted.reference) ?? null;
}

/** The source made on `candidate`. */
export function sourceOn(kind: "folder" | "code_folder" | "mail", candidate: SourceCandidate): NewSource {
  return { kind, label: candidate.label, reference: candidate.reference };
}

/** The source made on the file at `path`. */
export function sourceOnFile(path: string): NewSource {
  return { kind: "file", label: fileName(path), reference: path };
}

// ── Reading what the agent searches ─────────────────────────────────────

/** `path` with forward separators and no trailing one, folded on a Windows drive. */
function comparable(path: string): string {
  const forward = path.replace(/\\/g, "/").replace(/\/+$/, "");
  return /^[A-Za-z]:/.test(forward) ? forward.toLowerCase() : forward;
}

/** `true` when the file at `file` lies in the folder at `folder`, at any depth. */
export function liesIn(file: string, folder: string): boolean {
  const root = comparable(folder);
  return root !== "" && comparable(file).startsWith(`${root}/`);
}

/** The folder that holds the file at `path`; `null` for a bare name. */
export function parentFolder(path: string): string | null {
  const cut = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
  if (cut <= 0) return cut === 0 ? path.slice(0, 1) : null;
  return path.slice(0, cut);
}

/**
 * The folder connection a checked source is opened through: the one a
 * folder points at, the narrowest one holding a file. `null` otherwise.
 */
function connectionOf(row: SourceRow, connections: Connection[]): Connection | null {
  if (!row.checked || row.reference === null) return null;
  if (row.kind === "folder" || row.kind === "code_folder") {
    const bare = row.reference.startsWith("folder:") ? row.reference : `folder:${row.reference}`;
    return connections.find((c) => c.kind === "folder" && c.id === bare) ?? null;
  }
  if (row.kind !== "file") return null;
  const file = row.reference;
  const holding = connections.filter((c) => c.kind === "folder" && c.path != null && liesIn(file, c.path));
  holding.sort((a, b) => (b.path ?? "").length - (a.path ?? "").length);
  return holding[0] ?? null;
}

/** The reading tools of a folder connection. */
export function readingTools(connection: Connection): string[] {
  return connection.tools.filter((tool) => tool.family === "read").map((tool) => tool.id);
}

/**
 * The reading tools of the folder connection `row` is opened through that
 * the agent is not granted, when it is granted none of them: what "let it
 * open them" adds. A source is searched without them; opening one of its
 * files takes them. Empty when the row is not a checked folder or file,
 * when no connection holds it, or when the agent already reads it.
 */
export function readingToGrant(row: SourceRow, connections: Connection[], access: string[]): string[] {
  const connection = connectionOf(row, connections);
  if (!connection) return [];
  const reading = readingTools(connection);
  if (reading.some((id) => access.includes(id))) return [];
  return reading;
}

/**
 * The folder to connect so the agent can open the checked file `row`: the
 * one holding it, when no folder connection does. `null` for any other row.
 */
export function folderToConnect(row: SourceRow, connections: Connection[]): string | null {
  if (!row.checked || row.kind !== "file" || row.reference === null) return null;
  if (connectionOf(row, connections) !== null) return null;
  return parentFolder(row.reference);
}
